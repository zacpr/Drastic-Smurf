//! Cluster telemetry viewers: Kibana-style Log Viewer and time-series Metrics Viewer.
//!
//! Used both in Dashboard overview popup windows and as collapsible sections in the
//! single-cluster Detailed view.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui, Vec2};
use serde_json::{Value, json};

use crate::core::config::ClusterConfig;
use crate::ui::theme::Theme;
use crate::ui::widgets::{StatePill, draw_hud_brackets, human_bytes, human_docs};

/// Product filter shared by both Log Viewer and Metrics Viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TelemetryProductScope {
    #[default]
    Both,
    Elasticsearch,
    Kibana,
}

impl TelemetryProductScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Both => "Elastic + Kibana",
            Self::Elasticsearch => "Elasticsearch",
            Self::Kibana => "Kibana",
        }
    }
}

/// Timeframe presets (plus custom ISO UTC range) shared by Log Viewer and Metrics Viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TelemetryTimeframe {
    Last15m,
    Last30m,
    #[default]
    Last1h,
    Last6h,
    Last12h,
    Last24h,
    Last72h,
    Custom,
}

impl TelemetryTimeframe {
    pub const PRESETS: &'static [Self] = &[
        Self::Last15m,
        Self::Last30m,
        Self::Last1h,
        Self::Last6h,
        Self::Last12h,
        Self::Last24h,
        Self::Last72h,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Last15m => "Last 15m",
            Self::Last30m => "Last 30m",
            Self::Last1h => "Last 1h",
            Self::Last6h => "Last 6h",
            Self::Last12h => "Last 12h",
            Self::Last24h => "Last 24h",
            Self::Last72h => "Last 72h",
            Self::Custom => "Custom",
        }
    }

    pub fn fixed_interval(self, custom_from: &str, custom_to: &str) -> &'static str {
        match self {
            Self::Last15m => "30s",
            Self::Last30m | Self::Last1h => "1m",
            Self::Last6h => "5m",
            Self::Last12h => "10m",
            Self::Last24h => "15m",
            Self::Last72h => "1h",
            Self::Custom => {
                if let (Ok(from), Ok(to)) = (
                    DateTime::parse_from_rfc3339(custom_from.trim()),
                    DateTime::parse_from_rfc3339(custom_to.trim()),
                ) {
                    let mins = (to - from).num_minutes().abs();
                    if mins <= 30 {
                        "30s"
                    } else if mins <= 120 {
                        "1m"
                    } else if mins <= 720 {
                        "5m"
                    } else if mins <= 2880 {
                        "15m"
                    } else {
                        "1h"
                    }
                } else {
                    "5m"
                }
            }
        }
    }
}

/// Timeframe state shared by Log Viewer and Metrics Viewer.
#[derive(Debug, Clone)]
pub struct TelemetryTimeRange {
    pub preset: TelemetryTimeframe,
    pub custom_from: String,
    pub custom_to: String,
    pub show_custom_editor: bool,
}

impl Default for TelemetryTimeRange {
    fn default() -> Self {
        let now = Utc::now();
        let from = now - Duration::hours(1);
        Self {
            preset: TelemetryTimeframe::Last1h,
            custom_from: from.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            custom_to: now.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            show_custom_editor: false,
        }
    }
}

impl TelemetryTimeRange {
    pub fn range_bounds(&self) -> (String, String) {
        match self.preset {
            TelemetryTimeframe::Last15m => ("now-15m".to_string(), "now".to_string()),
            TelemetryTimeframe::Last30m => ("now-30m".to_string(), "now".to_string()),
            TelemetryTimeframe::Last1h => ("now-1h".to_string(), "now".to_string()),
            TelemetryTimeframe::Last6h => ("now-6h".to_string(), "now".to_string()),
            TelemetryTimeframe::Last12h => ("now-12h".to_string(), "now".to_string()),
            TelemetryTimeframe::Last24h => ("now-24h".to_string(), "now".to_string()),
            TelemetryTimeframe::Last72h => ("now-72h".to_string(), "now".to_string()),
            TelemetryTimeframe::Custom => {
                let from = if self.custom_from.trim().is_empty() {
                    "now-1h".to_string()
                } else {
                    self.custom_from.trim().to_string()
                };
                let to = if self.custom_to.trim().is_empty() {
                    "now".to_string()
                } else {
                    self.custom_to.trim().to_string()
                };
                (from, to)
            }
        }
    }

    pub fn fixed_interval(&self) -> &'static str {
        self.preset
            .fixed_interval(&self.custom_from, &self.custom_to)
    }

    pub fn summary_label(&self) -> String {
        if self.preset == TelemetryTimeframe::Custom {
            format!("🕒 Custom ({} .. {})", self.custom_from, self.custom_to)
        } else {
            format!("🕒 {}", self.preset.label())
        }
    }
}

/// Resolve a comma-separated index pattern for a chosen [`TelemetryProductScope`].
///
/// Returns `(resolved_pattern, product_filter_tag)`. If comma-separated segments
/// contain product-specific names (e.g. `logs-elasticsearch.*,logs-kibana.*`),
/// only the matching segments are kept.
pub fn resolve_telemetry_index_pattern(
    raw_pattern: &str,
    scope: TelemetryProductScope,
) -> (String, Option<&'static str>) {
    let parts: Vec<&str> = raw_pattern
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let full = if parts.is_empty() {
        "*".to_string()
    } else {
        parts.join(",")
    };

    match scope {
        TelemetryProductScope::Both => (full, None),
        TelemetryProductScope::Elasticsearch => {
            let matched: Vec<&str> = parts
                .iter()
                .copied()
                .filter(|seg| {
                    let lower = seg.to_ascii_lowercase();
                    lower.contains("elastic") && !lower.contains("kibana")
                })
                .collect();
            let pattern = if matched.is_empty() {
                full
            } else {
                matched.join(",")
            };
            (pattern, Some("elasticsearch"))
        }
        TelemetryProductScope::Kibana => {
            let matched: Vec<&str> = parts
                .iter()
                .copied()
                .filter(|seg| seg.to_ascii_lowercase().contains("kibana"))
                .collect();
            let pattern = if matched.is_empty() {
                full
            } else {
                matched.join(",")
            };
            (pattern, Some("kibana"))
        }
    }
}

fn product_filter_clause(product: &str) -> Value {
    json!({
        "bool": {
            "should": [
                { "term": { "service.type": product } },
                { "term": { "service.name": product } },
                { "term": { "event.module": product } },
                { "prefix": { "data_stream.dataset": product } },
                { "prefix": { "event.dataset": product } },
                { "wildcard": { "_index": format!("*{}*", product) } }
            ],
            "minimum_should_match": 1
        }
    })
}

fn pattern_was_narrowed(raw_pattern: &str, resolved_pattern: &str) -> bool {
    let normalized_raw: Vec<&str> = raw_pattern
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let normalized_resolved: Vec<&str> = resolved_pattern
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    normalized_raw != normalized_resolved
}

// ============================================================================
// LOG VIEWER
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevelFilter {
    #[default]
    All,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevelFilter {
    pub const ALL: &'static [Self] = &[
        Self::All,
        Self::Error,
        Self::Warn,
        Self::Info,
        Self::Debug,
        Self::Trace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All Levels",
            Self::Error => "Error / Fatal",
            Self::Warn => "Warn",
            Self::Info => "Info",
            Self::Debug => "Debug",
            Self::Trace => "Trace",
        }
    }

    fn es_terms(self) -> &'static [&'static str] {
        match self {
            Self::All => &[],
            Self::Error => &[
                "error", "ERROR", "err", "ERR", "fatal", "FATAL", "crit", "CRIT", "critical",
                "CRITICAL",
            ],
            Self::Warn => &["warn", "WARN", "warning", "WARNING"],
            Self::Info => &["info", "INFO"],
            Self::Debug => &["debug", "DEBUG"],
            Self::Trace => &["trace", "TRACE"],
        }
    }

    pub fn matches_level(self, raw_level: &str) -> bool {
        let lower = raw_level.trim().to_ascii_lowercase();
        match self {
            Self::All => true,
            Self::Error => matches!(
                lower.as_str(),
                "error" | "err" | "fatal" | "crit" | "critical" | "emergency" | "alert"
            ),
            Self::Warn => matches!(lower.as_str(), "warn" | "warning"),
            Self::Info => lower == "info",
            Self::Debug => lower == "debug",
            Self::Trace => lower == "trace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KibanaFilterKind {
    IncludeValue(String),
    ExcludeValue(String),
    IncludeExists,
    ExcludeExists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KibanaFieldFilter {
    pub field: String,
    pub kind: KibanaFilterKind,
    pub enabled: bool,
}

impl KibanaFieldFilter {
    pub fn include_value(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: KibanaFilterKind::IncludeValue(value.into()),
            enabled: true,
        }
    }

    pub fn exclude_value(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: KibanaFilterKind::ExcludeValue(value.into()),
            enabled: true,
        }
    }

    pub fn include_exists(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: KibanaFilterKind::IncludeExists,
            enabled: true,
        }
    }

    pub fn exclude_exists(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: KibanaFilterKind::ExcludeExists,
            enabled: true,
        }
    }

    pub fn is_exclude(&self) -> bool {
        matches!(
            self.kind,
            KibanaFilterKind::ExcludeValue(_) | KibanaFilterKind::ExcludeExists
        )
    }

    pub fn pill_label(&self) -> String {
        match &self.kind {
            KibanaFilterKind::IncludeValue(v) => format!("+ {}: \"{}\"", self.field, v),
            KibanaFilterKind::ExcludeValue(v) => format!("− {}: \"{}\"", self.field, v),
            KibanaFilterKind::IncludeExists => format!("+ exists: {}", self.field),
            KibanaFilterKind::ExcludeExists => format!("− exists: {}", self.field),
        }
    }

    pub fn matches_row(&self, row: &DashboardLogRow) -> bool {
        if !self.enabled {
            return true;
        }
        let field_val = row.field_value(&self.field);
        match &self.kind {
            KibanaFilterKind::IncludeValue(expected) => field_val.is_some_and(|v| v == expected),
            KibanaFilterKind::ExcludeValue(excluded) => field_val.is_none_or(|v| v != excluded),
            KibanaFilterKind::IncludeExists => field_val.is_some(),
            KibanaFilterKind::ExcludeExists => field_val.is_none(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterBuilderOp {
    #[default]
    Is,
    IsNot,
    Exists,
    DoesNotExist,
}

impl FilterBuilderOp {
    pub const ALL: &'static [Self] = &[Self::Is, Self::IsNot, Self::Exists, Self::DoesNotExist];

    pub fn label(self) -> &'static str {
        match self {
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::Exists => "exists",
            Self::DoesNotExist => "does not exist",
        }
    }

    pub fn needs_value(self) -> bool {
        matches!(self, Self::Is | Self::IsNot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogSortColumn {
    #[default]
    Timestamp,
    Level,
    Service,
    Host,
    Component,
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

impl SortOrder {
    pub fn flip(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    pub fn arrow(self) -> &'static str {
        match self {
            Self::Asc => "▲",
            Self::Desc => "▼",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DashboardLogRow {
    pub id: String,
    pub index: String,
    pub timestamp: String,
    pub level: String,
    pub service: String,
    pub host: String,
    pub component: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    pub raw_source: Value,
}

impl DashboardLogRow {
    pub fn field_value(&self, field: &str) -> Option<&str> {
        if field == "_index" {
            return Some(&self.index);
        }
        if field == "_id" {
            return Some(&self.id);
        }
        self.fields.get(field).map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub struct ClusterLogViewerState {
    pub product_scope: TelemetryProductScope,
    pub level_filter: LogLevelFilter,
    pub time_range: TelemetryTimeRange,
    pub search_query: String,
    pub filters: Vec<KibanaFieldFilter>,
    pub builder_field: String,
    pub builder_op: FilterBuilderOp,
    pub builder_value: String,
    pub sort_col: LogSortColumn,
    pub sort_order: SortOrder,
    pub rows: Vec<DashboardLogRow>,
    pub available_fields: Vec<String>,
    pub expanded_row_id: Option<String>,
    pub is_loading: bool,
    pub has_loaded: bool,
    pub error: Option<String>,
}

impl Default for ClusterLogViewerState {
    fn default() -> Self {
        Self {
            product_scope: TelemetryProductScope::Both,
            level_filter: LogLevelFilter::All,
            time_range: TelemetryTimeRange::default(),
            search_query: String::new(),
            filters: Vec::new(),
            builder_field: String::new(),
            builder_op: FilterBuilderOp::Is,
            builder_value: String::new(),
            sort_col: LogSortColumn::Timestamp,
            sort_order: SortOrder::Desc,
            rows: Vec::new(),
            available_fields: Vec::new(),
            expanded_row_id: None,
            is_loading: false,
            has_loaded: false,
            error: None,
        }
    }
}

impl ClusterLogViewerState {
    pub fn add_filter(&mut self, filter: KibanaFieldFilter) {
        if filter.field.trim().is_empty() {
            return;
        }
        if !self.filters.contains(&filter) {
            self.filters.push(filter);
        }
    }
}

/// Build the Elasticsearch `/_search` path and JSON query body for a cluster's log viewer.
pub fn build_dashboard_logs_query(
    config: &ClusterConfig,
    state: &ClusterLogViewerState,
) -> (String, Value) {
    let raw_pattern = config.effective_logs_index_pattern();
    let (resolved_pattern, product_tag) =
        resolve_telemetry_index_pattern(raw_pattern, state.product_scope);
    let path = format!("/{}/_search", resolved_pattern.trim_start_matches('/'));

    let (from, to) = state.time_range.range_bounds();
    let mut must: Vec<Value> = Vec::new();
    let mut filter: Vec<Value> = vec![json!({
        "range": {
            "@timestamp": {
                "gte": from,
                "lte": to
            }
        }
    })];
    let mut must_not: Vec<Value> = Vec::new();

    if let Some(product) = product_tag
        && !pattern_was_narrowed(raw_pattern, &resolved_pattern)
    {
        filter.push(product_filter_clause(product));
    }

    let level_terms = state.level_filter.es_terms();
    if !level_terms.is_empty() {
        filter.push(json!({
            "bool": {
                "should": [
                    { "terms": { "log.level": level_terms } },
                    { "terms": { "level": level_terms } },
                    { "terms": { "log.level.keyword": level_terms } }
                ],
                "minimum_should_match": 1
            }
        }));
    }

    if !state.search_query.trim().is_empty() {
        must.push(json!({
            "query_string": {
                "query": state.search_query.trim()
            }
        }));
    }

    for f in &state.filters {
        if !f.enabled {
            continue;
        }
        let field = f.field.trim();
        if field.is_empty() {
            continue;
        }
        match &f.kind {
            KibanaFilterKind::IncludeValue(val) => {
                filter.push(json!({
                    "match_phrase": { field: val }
                }));
            }
            KibanaFilterKind::ExcludeValue(val) => {
                must_not.push(json!({
                    "match_phrase": { field: val }
                }));
            }
            KibanaFilterKind::IncludeExists => {
                filter.push(json!({
                    "exists": { "field": field }
                }));
            }
            KibanaFilterKind::ExcludeExists => {
                must_not.push(json!({
                    "exists": { "field": field }
                }));
            }
        }
    }

    let body = json!({
        "size": 500,
        "sort": [
            { "@timestamp": { "order": "desc", "unmapped_type": "date" } }
        ],
        "query": {
            "bool": {
                "must": must,
                "filter": filter,
                "must_not": must_not
            }
        }
    });

    (path, body)
}

fn flatten_json_fields(val: &Value, prefix: &str, out: &mut BTreeMap<String, String>) {
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_json_fields(v, &path, out);
            }
        }
        Value::Array(arr) => {
            if arr.iter().all(|v| !v.is_object() && !v.is_array()) {
                let items: Vec<String> = arr.iter().map(scalar_to_string).collect();
                if !prefix.is_empty() {
                    out.insert(prefix.to_string(), items.join(", "));
                }
            } else {
                for item in arr {
                    flatten_json_fields(item, prefix, out);
                }
            }
        }
        Value::Null => {}
        _ => {
            if !prefix.is_empty() {
                out.insert(prefix.to_string(), scalar_to_string(val));
            }
        }
    }
}

fn scalar_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn first_field(fields: &BTreeMap<String, String>, candidates: &[&str]) -> String {
    for key in candidates {
        if let Some(val) = fields.get(*key)
            && !val.trim().is_empty()
        {
            return val.clone();
        }
    }
    String::new()
}

/// Parse an Elasticsearch `_search` response into normalized [`DashboardLogRow`] entries.
pub fn parse_dashboard_logs_response(val: &Value) -> Vec<DashboardLogRow> {
    let Some(hits) = val
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|h| h.as_array())
    else {
        return Vec::new();
    };

    hits.iter()
        .enumerate()
        .map(|(idx, hit)| {
            let id = hit
                .get("_id")
                .and_then(|v| v.as_str())
                .map(|s| format!("{}-{}", idx, s))
                .unwrap_or_else(|| format!("row-{}", idx));
            let index = hit
                .get("_index")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let raw_source = hit.get("_source").cloned().unwrap_or(Value::Null);
            let mut fields = BTreeMap::new();
            flatten_json_fields(&raw_source, "", &mut fields);
            if !index.is_empty() {
                fields.insert("_index".to_string(), index.clone());
            }

            let timestamp = first_field(&fields, &["@timestamp", "timestamp", "time"]);
            let level = first_field(
                &fields,
                &["log.level", "level", "severity", "log.syslog.severity.name"],
            );
            let mut service = first_field(
                &fields,
                &[
                    "service.type",
                    "service.name",
                    "event.module",
                    "data_stream.dataset",
                ],
            );
            if service.is_empty() {
                let lower_idx = index.to_ascii_lowercase();
                if lower_idx.contains("kibana") {
                    service = "kibana".to_string();
                } else if lower_idx.contains("elastic") {
                    service = "elasticsearch".to_string();
                }
            }
            let host = first_field(
                &fields,
                &[
                    "host.name",
                    "elasticsearch.node.name",
                    "host.hostname",
                    "agent.name",
                ],
            );
            let component = first_field(
                &fields,
                &[
                    "log.logger",
                    "elasticsearch.server.component",
                    "kibana.log.meta.type",
                    "event.dataset",
                ],
            );
            let message = first_field(
                &fields,
                &[
                    "message",
                    "log.original",
                    "error.message",
                    "elasticsearch.server.gc.overhead_duration",
                ],
            );

            DashboardLogRow {
                id,
                index,
                timestamp,
                level,
                service,
                host,
                component,
                message,
                fields,
                raw_source,
            }
        })
        .collect()
}

pub fn collect_available_fields(rows: &[DashboardLogRow]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for row in rows {
        for key in row.fields.keys() {
            set.insert(key.clone());
        }
    }
    set.into_iter().collect()
}

/// Filter and sort loaded log rows according to product scope, level, Kibana filters, and sort state.
pub fn visible_log_rows<'a>(state: &'a ClusterLogViewerState) -> Vec<&'a DashboardLogRow> {
    let mut rows: Vec<&'a DashboardLogRow> = state
        .rows
        .iter()
        .filter(|row| {
            // Product scope check
            match state.product_scope {
                TelemetryProductScope::Both => {}
                TelemetryProductScope::Elasticsearch => {
                    let svc = row.service.to_ascii_lowercase();
                    let idx = row.index.to_ascii_lowercase();
                    let ds = row
                        .field_value("data_stream.dataset")
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let is_es = svc.contains("elastic")
                        || idx.contains("elastic")
                        || ds.contains("elastic");
                    let is_kb_only = (svc.contains("kibana") || idx.contains("kibana")) && !is_es;
                    if is_kb_only || (!svc.is_empty() && !is_es) {
                        return false;
                    }
                }
                TelemetryProductScope::Kibana => {
                    let svc = row.service.to_ascii_lowercase();
                    let idx = row.index.to_ascii_lowercase();
                    let ds = row
                        .field_value("data_stream.dataset")
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let is_kb =
                        svc.contains("kibana") || idx.contains("kibana") || ds.contains("kibana");
                    if !is_kb {
                        return false;
                    }
                }
            }
            // Level check
            if state.level_filter != LogLevelFilter::All
                && !state.level_filter.matches_level(&row.level)
            {
                return false;
            }
            // Kibana field filters
            state.filters.iter().all(|f| f.matches_row(row))
        })
        .collect();

    rows.sort_by(|a, b| {
        let ord = match state.sort_col {
            LogSortColumn::Timestamp => a.timestamp.cmp(&b.timestamp),
            LogSortColumn::Level => a
                .level
                .to_ascii_lowercase()
                .cmp(&b.level.to_ascii_lowercase()),
            LogSortColumn::Service => a
                .service
                .to_ascii_lowercase()
                .cmp(&b.service.to_ascii_lowercase()),
            LogSortColumn::Host => a
                .host
                .to_ascii_lowercase()
                .cmp(&b.host.to_ascii_lowercase()),
            LogSortColumn::Component => a
                .component
                .to_ascii_lowercase()
                .cmp(&b.component.to_ascii_lowercase()),
            LogSortColumn::Message => a
                .message
                .to_ascii_lowercase()
                .cmp(&b.message.to_ascii_lowercase()),
        };
        match state.sort_order {
            SortOrder::Asc => ord,
            SortOrder::Desc => ord.reverse(),
        }
    });

    rows
}

fn render_product_scope_buttons(ui: &mut Ui, scope: &mut TelemetryProductScope) -> bool {
    let mut changed = false;
    for candidate in [
        TelemetryProductScope::Both,
        TelemetryProductScope::Elasticsearch,
        TelemetryProductScope::Kibana,
    ] {
        let active = *scope == candidate;
        let fill = if active {
            Theme::accent()
        } else {
            Theme::bg_input()
        };
        let text_color = if active {
            Theme::contrast_text_color(fill)
        } else {
            Theme::text_secondary()
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(candidate.label())
                        .size(11.0)
                        .color(text_color),
                )
                .fill(fill)
                .corner_radius(4.0),
            )
            .clicked()
            && !active
        {
            *scope = candidate;
            changed = true;
        }
    }
    changed
}

fn render_timeframe_picker(ui: &mut Ui, range: &mut TelemetryTimeRange, id_salt: &str) -> bool {
    let mut triggered = false;

    egui::ComboBox::from_id_salt((id_salt, "timeframe"))
        .selected_text(range.summary_label())
        .width(160.0)
        .show_ui(ui, |ui| {
            for &preset in TelemetryTimeframe::PRESETS {
                if ui
                    .selectable_label(range.preset == preset, preset.label())
                    .clicked()
                {
                    range.preset = preset;
                    range.show_custom_editor = false;
                    triggered = true;
                }
            }
            if ui
                .selectable_label(range.preset == TelemetryTimeframe::Custom, "Custom Range…")
                .clicked()
            {
                range.preset = TelemetryTimeframe::Custom;
                range.show_custom_editor = true;
            }
        });

    if range.preset == TelemetryTimeframe::Custom {
        let toggle_label = if range.show_custom_editor {
            "Hide Range"
        } else {
            "Edit Range"
        };
        if ui.small_button(toggle_label).clicked() {
            range.show_custom_editor = !range.show_custom_editor;
        }
    }

    triggered
}

fn render_custom_timeframe_editor(ui: &mut Ui, range: &mut TelemetryTimeRange) -> bool {
    if !(range.preset == TelemetryTimeframe::Custom && range.show_custom_editor) {
        return false;
    }
    let mut apply = false;
    egui::Frame::new()
        .fill(Theme::bg_input())
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(8))
        .stroke(Stroke::new(1.0_f32, Theme::accent().linear_multiply(0.35)))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("From (UTC ISO / ES math):").size(11.0));
                ui.add(
                    egui::TextEdit::singleline(&mut range.custom_from)
                        .desired_width(170.0)
                        .hint_text("2026-11-20T00:00:00Z or now-6h"),
                );
                ui.label(egui::RichText::new("To:").size(11.0));
                ui.add(
                    egui::TextEdit::singleline(&mut range.custom_to)
                        .desired_width(170.0)
                        .hint_text("2026-11-20T23:59:59Z or now"),
                );
                if ui.button("Apply Range").clicked() {
                    range.show_custom_editor = false;
                    apply = true;
                }
            });
        });
    apply
}

/// Render the Log Viewer panel for one cluster. Returns `true` when an ES query should run.
pub fn render_log_viewer_panel(
    ui: &mut Ui,
    config: &ClusterConfig,
    state: &mut ClusterLogViewerState,
    id_salt: &str,
    max_table_height: Option<f32>,
) -> bool {
    let mut trigger_fetch = false;

    let target_cluster = config.effective_telemetry_cluster();
    let (resolved_pattern, _) =
        resolve_telemetry_index_pattern(config.effective_logs_index_pattern(), state.product_scope);

    // Top metadata & controls bar
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .stroke(Stroke::new(1.0_f32, Theme::border()))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("Target cluster: {}", target_cluster))
                        .size(11.0)
                        .monospace()
                        .color(Theme::accent()),
                );
                ui.label(egui::RichText::new("•").color(Theme::text_muted()));
                ui.label(
                    egui::RichText::new(format!("Pattern: {}", resolved_pattern))
                        .size(11.0)
                        .monospace()
                        .color(Theme::text_secondary()),
                );
            });
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Source:").size(11.0).strong());
                if render_product_scope_buttons(ui, &mut state.product_scope) {
                    trigger_fetch = true;
                }

                ui.add_space(8.0);
                ui.label(egui::RichText::new("Level:").size(11.0).strong());
                let prev_level = state.level_filter;
                egui::ComboBox::from_id_salt((id_salt, "level_filter"))
                    .selected_text(state.level_filter.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for &lvl in LogLevelFilter::ALL {
                            ui.selectable_value(&mut state.level_filter, lvl, lvl.label());
                        }
                    });
                if state.level_filter != prev_level {
                    trigger_fetch = true;
                }

                ui.add_space(8.0);
                if render_timeframe_picker(ui, &mut state.time_range, id_salt) {
                    trigger_fetch = true;
                }
            });

            if render_custom_timeframe_editor(ui, &mut state.time_range) {
                trigger_fetch = true;
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Query:").size(11.0).strong());
                let q_resp = ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text("KQL / query_string, e.g. message:timeout OR status:500")
                        .desired_width((ui.available_width() - 210.0).max(160.0)),
                );
                let enter_pressed =
                    q_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                let search_label = if state.is_loading {
                    "⏳ Loading..."
                } else {
                    "🔍 Search"
                };
                if ui
                    .add_enabled(!state.is_loading, egui::Button::new(search_label))
                    .clicked()
                    || enter_pressed
                {
                    trigger_fetch = true;
                }
                if !state.search_query.is_empty() && ui.small_button("Clear Query").clicked() {
                    state.search_query.clear();
                    trigger_fetch = true;
                }
            });

            // Kibana-style field filter builder & active filter pills
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Add Filter:")
                        .size(11.0)
                        .color(Theme::text_secondary()),
                );
                if !state.available_fields.is_empty() {
                    let preview = if state.builder_field.is_empty() {
                        "Pick field…"
                    } else {
                        &state.builder_field
                    };
                    egui::ComboBox::from_id_salt((id_salt, "field_suggest"))
                        .selected_text(preview)
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for field in &state.available_fields {
                                ui.selectable_value(&mut state.builder_field, field.clone(), field);
                            }
                        });
                }
                ui.add(
                    egui::TextEdit::singleline(&mut state.builder_field)
                        .hint_text("field (e.g. host.name)")
                        .desired_width(140.0),
                );
                egui::ComboBox::from_id_salt((id_salt, "filter_op"))
                    .selected_text(state.builder_op.label())
                    .width(115.0)
                    .show_ui(ui, |ui| {
                        for &op in FilterBuilderOp::ALL {
                            ui.selectable_value(&mut state.builder_op, op, op.label());
                        }
                    });
                if state.builder_op.needs_value() {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.builder_value)
                            .hint_text("value")
                            .desired_width(130.0),
                    );
                }
                let can_add = !state.builder_field.trim().is_empty()
                    && (!state.builder_op.needs_value() || !state.builder_value.trim().is_empty());
                if ui
                    .add_enabled(can_add, egui::Button::new("➕ Add Filter"))
                    .clicked()
                {
                    let field = state.builder_field.trim().to_string();
                    let val = state.builder_value.trim().to_string();
                    let new_filter = match state.builder_op {
                        FilterBuilderOp::Is => KibanaFieldFilter::include_value(field, val),
                        FilterBuilderOp::IsNot => KibanaFieldFilter::exclude_value(field, val),
                        FilterBuilderOp::Exists => KibanaFieldFilter::include_exists(field),
                        FilterBuilderOp::DoesNotExist => KibanaFieldFilter::exclude_exists(field),
                    };
                    state.add_filter(new_filter);
                    state.builder_value.clear();
                }
            });

            if !state.filters.is_empty() {
                ui.add_space(4.0);
                let mut remove_idx: Option<usize> = None;
                let mut toggle_idx: Option<usize> = None;
                let mut clear_all = false;

                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Active Filters:")
                            .size(10.5)
                            .color(Theme::text_muted()),
                    );
                    for (idx, f) in state.filters.iter().enumerate() {
                        let base_color = if !f.enabled {
                            Theme::text_muted()
                        } else if f.is_exclude() {
                            Theme::danger()
                        } else {
                            Theme::success()
                        };
                        egui::Frame::new()
                            .fill(base_color.linear_multiply(0.18))
                            .stroke(Stroke::new(1.0_f32, base_color))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let text = egui::RichText::new(f.pill_label())
                                        .size(10.5)
                                        .color(if f.enabled {
                                            Theme::text_primary()
                                        } else {
                                            Theme::text_muted()
                                        });
                                    if ui
                                        .add(egui::Button::new(text).frame(false))
                                        .on_hover_text("Click to temporarily disable/enable filter")
                                        .clicked()
                                    {
                                        toggle_idx = Some(idx);
                                    }
                                    if ui
                                        .small_button("×")
                                        .on_hover_text("Remove filter")
                                        .clicked()
                                    {
                                        remove_idx = Some(idx);
                                    }
                                });
                            });
                    }
                    if ui.small_button("Clear filters").clicked() {
                        clear_all = true;
                    }
                    if ui
                        .small_button("↻ Apply to ES Query")
                        .on_hover_text("Re-run the Elasticsearch query with active filters")
                        .clicked()
                    {
                        trigger_fetch = true;
                    }
                });

                if let Some(idx) = toggle_idx {
                    state.filters[idx].enabled = !state.filters[idx].enabled;
                }
                if let Some(idx) = remove_idx {
                    state.filters.remove(idx);
                }
                if clear_all {
                    state.filters.clear();
                }
            }
        });

    ui.add_space(6.0);

    if let Some(ref err) = state.error {
        ui.colored_label(Theme::danger(), format!("⚠ {}", err));
        ui.add_space(4.0);
    }

    let visible_indices: Vec<usize> = {
        let ptrs = visible_log_rows(state);
        let row_base = state.rows.as_ptr() as usize;
        ptrs.into_iter()
            .map(|r| {
                let addr = (r as *const DashboardLogRow) as usize;
                (addr - row_base) / std::mem::size_of::<DashboardLogRow>()
            })
            .collect()
    };

    ui.horizontal(|ui| {
        if state.is_loading {
            ui.spinner();
            ui.label(egui::RichText::new("Fetching logs…").color(Theme::text_muted()));
        } else {
            ui.label(
                egui::RichText::new(format!(
                    "Showing {} of {} fetched log entries",
                    visible_indices.len(),
                    state.rows.len()
                ))
                .size(11.0)
                .color(Theme::text_muted()),
            );
        }
    });
    ui.add_space(4.0);

    // Tabular log rows with sortable column headers and inline Kibana field drawers
    let mut pending_filter: Option<KibanaFieldFilter> = None;
    let mut sort_clicked: Option<LogSortColumn> = None;
    let mut expanded_id = state.expanded_row_id.clone();

    let table_h = max_table_height.unwrap_or_else(|| (ui.available_height() - 10.0).max(220.0));
    let mut scroll = egui::ScrollArea::both()
        .id_salt((id_salt, "logs_table_scroll"))
        .auto_shrink([false, false]);
    if max_table_height.is_some() {
        scroll = scroll.max_height(table_h);
    } else {
        scroll = scroll.min_scrolled_height(table_h).max_height(table_h);
    }

    scroll.show(ui, |ui| {
        // Header row
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            for (col, label, w) in [
                (LogSortColumn::Timestamp, "Timestamp", 165.0),
                (LogSortColumn::Level, "Level", 70.0),
                (LogSortColumn::Service, "Service", 105.0),
                (LogSortColumn::Host, "Host / Node", 135.0),
                (LogSortColumn::Component, "Component", 150.0),
                (LogSortColumn::Message, "Message", 420.0),
            ] {
                let text = if state.sort_col == col {
                    format!("{} {}", label, state.sort_order.arrow())
                } else {
                    label.to_string()
                };
                ui.allocate_ui(Vec2::new(w, 22.0), |ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(text).strong().size(11.0).color(
                                if state.sort_col == col {
                                    Theme::accent()
                                } else {
                                    Theme::text_secondary()
                                },
                            ))
                            .frame(false),
                        )
                        .clicked()
                    {
                        sort_clicked = Some(col);
                    }
                });
            }
        });
        ui.separator();

        if visible_indices.is_empty() && !state.is_loading {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("No log entries match the current timeframe and filters.")
                    .color(Theme::text_muted()),
            );
        }

        for (display_idx, &row_idx) in visible_indices.iter().enumerate() {
            let row = &state.rows[row_idx];
            let is_expanded = expanded_id.as_deref() == Some(row.id.as_str());
            let row_bg = if display_idx % 2 == 0 {
                Theme::bg_input().linear_multiply(0.55)
            } else {
                Color32::TRANSPARENT
            };

            egui::Frame::new()
                .fill(row_bg)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let arrow = if is_expanded { "▼" } else { "▶" };
                        if ui.small_button(arrow).clicked() {
                            expanded_id = if is_expanded {
                                None
                            } else {
                                Some(row.id.clone())
                            };
                        }

                        ui.allocate_ui(Vec2::new(165.0, 20.0), |ui| {
                            ui.label(
                                egui::RichText::new(if row.timestamp.is_empty() {
                                    "—"
                                } else {
                                    &row.timestamp
                                })
                                .monospace()
                                .size(10.5)
                                .color(Theme::text_muted()),
                            );
                        });

                        ui.allocate_ui(Vec2::new(70.0, 20.0), |ui| {
                            let lvl_lower = row.level.to_ascii_lowercase();
                            let (badge_txt, badge_col) = match lvl_lower.as_str() {
                                "error" | "err" | "fatal" | "crit" | "critical" => {
                                    ("ERR", Theme::danger())
                                }
                                "warn" | "warning" => ("WRN", Theme::warning()),
                                "info" => ("INF", Theme::success()),
                                "debug" => ("DBG", Theme::text_secondary()),
                                "trace" => ("TRC", Theme::text_muted()),
                                "" => ("—", Theme::text_muted()),
                                _ => (row.level.as_str(), Theme::text_secondary()),
                            };
                            let resp = ui.add(StatePill::new(badge_txt, badge_col));
                            if !row.level.is_empty() {
                                cell_filter_context_menu(
                                    &resp,
                                    "log.level",
                                    &row.level,
                                    &mut pending_filter,
                                );
                            }
                        });

                        ui.allocate_ui(Vec2::new(105.0, 20.0), |ui| {
                            let resp = ui.label(
                                egui::RichText::new(if row.service.is_empty() {
                                    "—"
                                } else {
                                    &row.service
                                })
                                .size(11.0)
                                .color(Theme::accent()),
                            );
                            if !row.service.is_empty() {
                                cell_filter_context_menu(
                                    &resp,
                                    "service.type",
                                    &row.service,
                                    &mut pending_filter,
                                );
                            }
                        });

                        ui.allocate_ui(Vec2::new(135.0, 20.0), |ui| {
                            let resp = ui.label(
                                egui::RichText::new(if row.host.is_empty() {
                                    "—"
                                } else {
                                    &row.host
                                })
                                .size(11.0)
                                .color(Theme::text_secondary()),
                            );
                            if !row.host.is_empty() {
                                cell_filter_context_menu(
                                    &resp,
                                    "host.name",
                                    &row.host,
                                    &mut pending_filter,
                                );
                            }
                        });

                        ui.allocate_ui(Vec2::new(150.0, 20.0), |ui| {
                            let comp_short = truncate_chars(&row.component, 22);
                            let resp = ui
                                .label(
                                    egui::RichText::new(if comp_short.is_empty() {
                                        "—".to_string()
                                    } else {
                                        comp_short
                                    })
                                    .size(10.5)
                                    .color(Theme::text_muted()),
                                )
                                .on_hover_text(&row.component);
                            if !row.component.is_empty() {
                                cell_filter_context_menu(
                                    &resp,
                                    "log.logger",
                                    &row.component,
                                    &mut pending_filter,
                                );
                            }
                        });

                        let msg_short = truncate_chars(&row.message, 120);
                        ui.label(
                            egui::RichText::new(if msg_short.is_empty() {
                                "(no message)".to_string()
                            } else {
                                msg_short
                            })
                            .size(11.0)
                            .color(Theme::text_primary()),
                        );
                    });

                    if is_expanded {
                        ui.add_space(4.0);
                        ui.indent((&row.id, "drawer"), |ui| {
                            render_expanded_log_fields(ui, row, &mut pending_filter);
                        });
                        ui.add_space(4.0);
                    }
                });
        }
    });

    state.expanded_row_id = expanded_id;
    if let Some(col) = sort_clicked {
        if state.sort_col == col {
            state.sort_order = state.sort_order.flip();
        } else {
            state.sort_col = col;
            state.sort_order = SortOrder::Desc;
        }
    }
    if let Some(f) = pending_filter {
        state.add_filter(f);
    }

    trigger_fetch
}

fn cell_filter_context_menu(
    resp: &egui::Response,
    field: &str,
    value: &str,
    pending_filter: &mut Option<KibanaFieldFilter>,
) {
    resp.context_menu(|ui| {
        if ui
            .button(format!("➕ Filter for {}: \"{}\"", field, value))
            .clicked()
        {
            *pending_filter = Some(KibanaFieldFilter::include_value(field, value));
            ui.close_menu();
        }
        if ui
            .button(format!("➖ Filter out {}: \"{}\"", field, value))
            .clicked()
        {
            *pending_filter = Some(KibanaFieldFilter::exclude_value(field, value));
            ui.close_menu();
        }
        if ui
            .button(format!("∃ Filter for {} exists", field))
            .clicked()
        {
            *pending_filter = Some(KibanaFieldFilter::include_exists(field));
            ui.close_menu();
        }
        if ui.button(format!("∄ Exclude {} exists", field)).clicked() {
            *pending_filter = Some(KibanaFieldFilter::exclude_exists(field));
            ui.close_menu();
        }
    });
}

fn render_expanded_log_fields(
    ui: &mut Ui,
    row: &DashboardLogRow,
    pending_filter: &mut Option<KibanaFieldFilter>,
) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(8))
        .stroke(Stroke::new(1.0_f32, Theme::accent().linear_multiply(0.3)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        "Document Fields (click ➕ / ➖ for value, ∃ / ∄ for presence)",
                    )
                    .size(11.0)
                    .strong()
                    .color(Theme::accent()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("📋 Copy JSON").clicked() {
                        let pretty =
                            serde_json::to_string_pretty(&row.raw_source).unwrap_or_default();
                        ui.ctx().copy_text(pretty);
                    }
                });
            });
            ui.add_space(4.0);

            egui::Grid::new((&row.id, "fields_grid"))
                .num_columns(3)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    for (field, value) in &row.fields {
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("➕")
                                .on_hover_text(format!("Include {} == \"{}\"", field, value))
                                .clicked()
                            {
                                *pending_filter =
                                    Some(KibanaFieldFilter::include_value(field, value));
                            }
                            if ui
                                .small_button("➖")
                                .on_hover_text(format!("Exclude {} == \"{}\"", field, value))
                                .clicked()
                            {
                                *pending_filter =
                                    Some(KibanaFieldFilter::exclude_value(field, value));
                            }
                            if ui
                                .small_button("∃")
                                .on_hover_text(format!("Include docs where {} exists", field))
                                .clicked()
                            {
                                *pending_filter = Some(KibanaFieldFilter::include_exists(field));
                            }
                            if ui
                                .small_button("∄")
                                .on_hover_text(format!("Exclude docs where {} exists", field))
                                .clicked()
                            {
                                *pending_filter = Some(KibanaFieldFilter::exclude_exists(field));
                            }
                        });
                        ui.label(
                            egui::RichText::new(field)
                                .monospace()
                                .size(10.5)
                                .color(Theme::text_secondary()),
                        );
                        ui.label(
                            egui::RichText::new(value)
                                .monospace()
                                .size(10.5)
                                .color(Theme::text_primary()),
                        );
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            ui.collapsing("Raw JSON", |ui| {
                let pretty = serde_json::to_string_pretty(&row.raw_source).unwrap_or_default();
                ui.add(
                    egui::TextEdit::multiline(&mut pretty.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
            });
        });
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

// ============================================================================
// METRICS VIEWER
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricUnit {
    Percent,
    Bytes,
    Millis,
    Count,
    Float,
}

impl MetricUnit {
    pub fn format_value(self, value: f64) -> String {
        match self {
            Self::Percent => format!("{:.1}%", value),
            Self::Bytes => human_bytes(value.max(0.0) as u64),
            Self::Millis => format!("{:.1} ms", value),
            Self::Count => {
                if value >= 1000.0 {
                    human_docs(value as u64)
                } else if (value.fract()).abs() < 0.05 {
                    format!("{:.0}", value)
                } else {
                    format!("{:.1}", value)
                }
            }
            Self::Float => format!("{:.2}", value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricPoint {
    pub timestamp_ms: i64,
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct MetricChartSeries {
    pub id: String,
    pub title: String,
    pub product: TelemetryProductScope,
    pub unit: MetricUnit,
    pub points: Vec<MetricPoint>,
}

#[derive(Debug, Clone, Default)]
pub struct ClusterMetricsData {
    pub series: Vec<MetricChartSeries>,
    pub total_docs: usize,
}

#[derive(Debug, Clone)]
pub struct ClusterMetricsViewerState {
    pub product_scope: TelemetryProductScope,
    pub time_range: TelemetryTimeRange,
    pub data: ClusterMetricsData,
    pub is_loading: bool,
    pub has_loaded: bool,
    pub error: Option<String>,
}

impl Default for ClusterMetricsViewerState {
    fn default() -> Self {
        Self {
            product_scope: TelemetryProductScope::Both,
            time_range: TelemetryTimeRange::default(),
            data: ClusterMetricsData::default(),
            is_loading: false,
            has_loaded: false,
            error: None,
        }
    }
}

struct CuratedMetricSpec {
    id: &'static str,
    title: &'static str,
    product: TelemetryProductScope,
    unit: MetricUnit,
    candidates: &'static [(&'static str, f64)],
}

const CURATED_METRICS: &[CuratedMetricSpec] = &[
    CuratedMetricSpec {
        id: "es_cpu_pct",
        title: "ES CPU Usage",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Percent,
        candidates: &[
            ("elasticsearch.node.stats.os.cpu.percent", 1.0),
            ("system.process.cpu.total.norm.pct", 100.0),
            ("system.cpu.total.norm.pct", 100.0),
            ("host.cpu.usage", 100.0),
        ],
    },
    CuratedMetricSpec {
        id: "es_jvm_heap_pct",
        title: "ES JVM Heap Used (%)",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Percent,
        candidates: &[("elasticsearch.node.stats.jvm.mem.heap_used_percent", 1.0)],
    },
    CuratedMetricSpec {
        id: "es_jvm_heap_bytes",
        title: "ES JVM Heap Used",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Bytes,
        candidates: &[
            ("elasticsearch.node.stats.jvm.mem.heap_used_in_bytes", 1.0),
            ("jvm.mem.heap_used_in_bytes", 1.0),
        ],
    },
    CuratedMetricSpec {
        id: "es_disk_avail",
        title: "ES Disk Available",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Bytes,
        candidates: &[
            (
                "elasticsearch.node.stats.fs.summary.total.available_in_bytes",
                1.0,
            ),
            ("elasticsearch.node.stats.fs.total.available_in_bytes", 1.0),
        ],
    },
    CuratedMetricSpec {
        id: "es_docs_count",
        title: "ES Document Count",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Count,
        candidates: &[
            ("elasticsearch.node.stats.indices.docs.count", 1.0),
            ("elasticsearch.index.summary.total.docs.count", 1.0),
        ],
    },
    CuratedMetricSpec {
        id: "es_indexing_total",
        title: "ES Indexing Operations",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Count,
        candidates: &[("elasticsearch.node.stats.indices.indexing.index_total", 1.0)],
    },
    CuratedMetricSpec {
        id: "es_search_total",
        title: "ES Search Query Operations",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Count,
        candidates: &[("elasticsearch.node.stats.indices.search.query_total", 1.0)],
    },
    CuratedMetricSpec {
        id: "es_search_time",
        title: "ES Cumulative Search Time",
        product: TelemetryProductScope::Elasticsearch,
        unit: MetricUnit::Millis,
        candidates: &[(
            "elasticsearch.node.stats.indices.search.query_time_in_millis",
            1.0,
        )],
    },
    CuratedMetricSpec {
        id: "kb_heap_used",
        title: "Kibana Heap Memory Used",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Bytes,
        candidates: &[
            ("kibana.stats.process.memory.heap.used.bytes", 1.0),
            ("kibana.stats.process.mem.heap_used_in_bytes", 1.0),
        ],
    },
    CuratedMetricSpec {
        id: "kb_resp_avg",
        title: "Kibana Response Time (Avg)",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Millis,
        candidates: &[("kibana.stats.response_times.avg_in_ms", 1.0)],
    },
    CuratedMetricSpec {
        id: "kb_resp_max",
        title: "Kibana Response Time (Max)",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Millis,
        candidates: &[("kibana.stats.response_times.max_in_ms", 1.0)],
    },
    CuratedMetricSpec {
        id: "kb_requests_total",
        title: "Kibana Requests Total",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Count,
        candidates: &[("kibana.stats.requests.total", 1.0)],
    },
    CuratedMetricSpec {
        id: "kb_concurrent_conns",
        title: "Kibana Concurrent Connections",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Count,
        candidates: &[("kibana.stats.concurrent_connections", 1.0)],
    },
    CuratedMetricSpec {
        id: "kb_event_loop_delay",
        title: "Kibana Event Loop Delay",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Millis,
        candidates: &[("kibana.stats.process.event_loop_delay", 1.0)],
    },
    CuratedMetricSpec {
        id: "kb_os_load_1m",
        title: "Kibana OS Load (1m)",
        product: TelemetryProductScope::Kibana,
        unit: MetricUnit::Float,
        candidates: &[("kibana.stats.os.load.1m", 1.0), ("system.load.1", 1.0)],
    },
];

/// Build the Elasticsearch `/_search` query + `date_histogram` aggregations for metrics.
pub fn build_dashboard_metrics_query(
    config: &ClusterConfig,
    state: &ClusterMetricsViewerState,
) -> (String, Value) {
    let raw_pattern = config.effective_metrics_index_pattern();
    let (resolved_pattern, product_tag) =
        resolve_telemetry_index_pattern(raw_pattern, state.product_scope);
    let path = format!("/{}/_search", resolved_pattern.trim_start_matches('/'));

    let (from, to) = state.time_range.range_bounds();
    let interval = state.time_range.fixed_interval();
    let mut filter: Vec<Value> = vec![json!({
        "range": {
            "@timestamp": {
                "gte": from,
                "lte": to
            }
        }
    })];

    if let Some(product) = product_tag
        && !pattern_was_narrowed(raw_pattern, &resolved_pattern)
    {
        filter.push(product_filter_clause(product));
    }

    let mut sub_aggs = serde_json::Map::new();
    for spec in CURATED_METRICS {
        if state.product_scope != TelemetryProductScope::Both && spec.product != state.product_scope
        {
            continue;
        }
        for (idx, (field, _)) in spec.candidates.iter().enumerate() {
            let agg_key = format!("{}__{}", spec.id, idx);
            sub_aggs.insert(
                agg_key,
                json!({
                    "avg": { "field": field }
                }),
            );
        }
    }

    let body = json!({
        "size": 300,
        "sort": [
            { "@timestamp": { "order": "desc", "unmapped_type": "date" } }
        ],
        "query": {
            "bool": {
                "filter": filter
            }
        },
        "aggs": {
            "by_time": {
                "date_histogram": {
                    "field": "@timestamp",
                    "fixed_interval": interval,
                    "min_doc_count": 1
                },
                "aggs": Value::Object(sub_aggs)
            }
        }
    });

    (path, body)
}

fn format_ts_label(ts_ms: i64, fallback: &str) -> String {
    if let Some(dt) = DateTime::from_timestamp_millis(ts_ms) {
        dt.format("%H:%M:%S").to_string()
    } else if fallback.len() >= 19 {
        fallback[11..19].to_string()
    } else {
        fallback.to_string()
    }
}

fn parse_ts_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn get_dot_value<'a>(val: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = val;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

fn collect_numeric_leaves(val: &Value, prefix: &str, out: &mut BTreeMap<String, f64>) {
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                collect_numeric_leaves(v, &next, out);
            }
        }
        Value::Number(n) if !prefix.is_empty() => {
            if let Some(f) = n.as_f64()
                && f.is_finite()
            {
                out.insert(prefix.to_string(), f);
            }
        }
        _ => {}
    }
}

/// Parse an Elasticsearch `_search` + `date_histogram` metrics response into chart series.
pub fn parse_dashboard_metrics_response(val: &Value) -> ClusterMetricsData {
    let mut series: Vec<MetricChartSeries> = Vec::new();
    let buckets = val
        .pointer("/aggregations/by_time/buckets")
        .and_then(|b| b.as_array());

    // 1. Extract curated series from date_histogram buckets first
    if let Some(buckets) = buckets
        && !buckets.is_empty()
    {
        for spec in CURATED_METRICS {
            for (idx, (_, scale)) in spec.candidates.iter().enumerate() {
                let agg_key = format!("{}__{}", spec.id, idx);
                let mut points = Vec::new();
                for bucket in buckets {
                    if let Some(raw_val) = bucket
                        .get(&agg_key)
                        .and_then(|a| a.get("value"))
                        .and_then(|v| v.as_f64())
                        && raw_val.is_finite()
                    {
                        let ts_ms = bucket.get("key").and_then(|k| k.as_i64()).unwrap_or(0);
                        let key_str = bucket
                            .get("key_as_string")
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        points.push(MetricPoint {
                            timestamp_ms: ts_ms,
                            label: format_ts_label(ts_ms, key_str),
                            value: raw_val * scale,
                        });
                    }
                }
                if !points.is_empty() {
                    series.push(MetricChartSeries {
                        id: spec.id.to_string(),
                        title: spec.title.to_string(),
                        product: spec.product,
                        unit: spec.unit,
                        points,
                    });
                    break;
                }
            }
        }
    }

    // 2. Fallback / supplement from raw hits in chronological order
    let hits = val
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();
    let total_docs = hits.len();

    if !hits.is_empty() {
        let mut chronological: Vec<(i64, String, &Value, &str)> = hits
            .iter()
            .enumerate()
            .filter_map(|(idx, hit)| {
                let source = hit.get("_source")?;
                let index = hit.get("_index").and_then(|i| i.as_str()).unwrap_or("");
                let ts_str = source
                    .get("@timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ts_ms = parse_ts_ms(ts_str).unwrap_or(idx as i64);
                let label = format_ts_label(ts_ms, ts_str);
                Some((ts_ms, label, source, index))
            })
            .collect();
        chronological.sort_by_key(|(ts_ms, _, _, _)| *ts_ms);

        for spec in CURATED_METRICS {
            if series.iter().any(|s| s.id == spec.id) {
                continue;
            }
            for &(field, scale) in spec.candidates {
                let mut points = Vec::new();
                for &(ts_ms, ref label, source, _) in &chronological {
                    if let Some(raw_val) = get_dot_value(source, field).and_then(|v| v.as_f64())
                        && raw_val.is_finite()
                    {
                        points.push(MetricPoint {
                            timestamp_ms: ts_ms,
                            label: label.clone(),
                            value: raw_val * scale,
                        });
                    }
                }
                if !points.is_empty() {
                    series.push(MetricChartSeries {
                        id: spec.id.to_string(),
                        title: spec.title.to_string(),
                        product: spec.product,
                        unit: spec.unit,
                        points,
                    });
                    break;
                }
            }
        }

        // 3. If no curated metrics matched at all (custom schema), discover up to 8 numeric fields
        if series.is_empty() {
            let mut by_field: BTreeMap<String, (TelemetryProductScope, Vec<MetricPoint>)> =
                BTreeMap::new();
            for &(ts_ms, ref label, source, index) in &chronological {
                let mut leaves = BTreeMap::new();
                collect_numeric_leaves(source, "", &mut leaves);
                for (path, v) in leaves {
                    if path.ends_with("port") || path.ends_with("pid") {
                        continue;
                    }
                    let lower_path = path.to_ascii_lowercase();
                    let lower_idx = index.to_ascii_lowercase();
                    let product = if lower_path.contains("kibana") || lower_idx.contains("kibana") {
                        TelemetryProductScope::Kibana
                    } else if lower_path.contains("elastic") || lower_idx.contains("elastic") {
                        TelemetryProductScope::Elasticsearch
                    } else {
                        TelemetryProductScope::Both
                    };
                    let entry = by_field
                        .entry(path)
                        .or_insert_with(|| (product, Vec::new()));
                    entry.1.push(MetricPoint {
                        timestamp_ms: ts_ms,
                        label: label.clone(),
                        value: v,
                    });
                }
            }
            for (path, (product, mut points)) in by_field.into_iter().take(8) {
                points.sort_by_key(|p| p.timestamp_ms);
                let unit = if path.contains("bytes") {
                    MetricUnit::Bytes
                } else if path.contains("percent") || path.contains("pct") {
                    MetricUnit::Percent
                } else if path.contains("millis") || path.contains("_ms") {
                    MetricUnit::Millis
                } else {
                    MetricUnit::Float
                };
                series.push(MetricChartSeries {
                    id: path.clone(),
                    title: path,
                    product,
                    unit,
                    points,
                });
            }
        }
    }

    for item in &mut series {
        item.points.sort_by_key(|p| p.timestamp_ms);
    }

    ClusterMetricsData { series, total_docs }
}

/// Render the Metrics Viewer panel for one cluster. Returns `true` when an ES refresh is requested.
pub fn render_metrics_viewer_panel(
    ui: &mut Ui,
    config: &ClusterConfig,
    state: &mut ClusterMetricsViewerState,
    id_salt: &str,
) -> bool {
    let mut trigger_fetch = false;

    let target_cluster = config.effective_telemetry_cluster();
    let (resolved_pattern, _) = resolve_telemetry_index_pattern(
        config.effective_metrics_index_pattern(),
        state.product_scope,
    );

    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .stroke(Stroke::new(1.0_f32, Theme::border()))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("Target cluster: {}", target_cluster))
                        .size(11.0)
                        .monospace()
                        .color(Theme::accent()),
                );
                ui.label(egui::RichText::new("•").color(Theme::text_muted()));
                ui.label(
                    egui::RichText::new(format!("Pattern: {}", resolved_pattern))
                        .size(11.0)
                        .monospace()
                        .color(Theme::text_secondary()),
                );
                if state.has_loaded {
                    ui.label(egui::RichText::new("•").color(Theme::text_muted()));
                    ui.label(
                        egui::RichText::new(format!("{} sampled docs", state.data.total_docs))
                            .size(11.0)
                            .color(Theme::text_muted()),
                    );
                }
            });
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Source:").size(11.0).strong());
                if render_product_scope_buttons(ui, &mut state.product_scope) {
                    trigger_fetch = true;
                }

                ui.add_space(8.0);
                if render_timeframe_picker(ui, &mut state.time_range, id_salt) {
                    trigger_fetch = true;
                }

                ui.add_space(8.0);
                let btn_label = if state.is_loading {
                    "⏳ Loading..."
                } else {
                    "🔄 Refresh Metrics"
                };
                if ui
                    .add_enabled(!state.is_loading, egui::Button::new(btn_label))
                    .clicked()
                {
                    trigger_fetch = true;
                }
            });

            if render_custom_timeframe_editor(ui, &mut state.time_range) {
                trigger_fetch = true;
            }
        });

    ui.add_space(8.0);

    if let Some(ref err) = state.error {
        ui.colored_label(Theme::danger(), format!("⚠ {}", err));
        ui.add_space(6.0);
    }

    let filtered_series: Vec<&MetricChartSeries> = state
        .data
        .series
        .iter()
        .filter(|s| {
            state.product_scope == TelemetryProductScope::Both
                || s.product == TelemetryProductScope::Both
                || s.product == state.product_scope
        })
        .collect();

    if state.is_loading && filtered_series.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Loading metric time series…").color(Theme::text_muted()));
        });
        return trigger_fetch;
    }

    if filtered_series.is_empty() {
        ui.label(
            egui::RichText::new(
                "No metric series found for this cluster/timeframe. Check Metrics Index Pattern or timeframe.",
            )
            .color(Theme::text_muted()),
        );
        return trigger_fetch;
    }

    let available_w = (ui.available_width() - 8.0).max(260.0);
    let spacing = 12.0;
    let cols = if available_w >= 760.0 { 2 } else { 1 };
    let card_w = ((available_w - (cols - 1) as f32 * spacing) / cols as f32).floor();

    for chunk in filtered_series.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for series in chunk {
                render_metric_chart_card(ui, series, card_w);
            }
        });
        ui.add_space(spacing);
    }

    trigger_fetch
}

fn render_metric_chart_card(ui: &mut Ui, series: &MetricChartSeries, card_w: f32) {
    let accent = match series.product {
        TelemetryProductScope::Elasticsearch => Theme::accent(),
        TelemetryProductScope::Kibana => Theme::success(),
        TelemetryProductScope::Both => Theme::warning(),
    };
    let product_badge = match series.product {
        TelemetryProductScope::Elasticsearch => "Elasticsearch",
        TelemetryProductScope::Kibana => "Kibana",
        TelemetryProductScope::Both => "Metric",
    };

    let values: Vec<f64> = series.points.iter().map(|p| p.value).collect();
    let min_v = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_v = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg_v = if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    };
    let latest_v = values.last().copied().unwrap_or(0.0);

    let frame_resp = egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .stroke(Stroke::new(1.0_f32, accent.linear_multiply(0.28)))
        .show(ui, |ui| {
            let inner_w = (card_w - Theme::CARD_PADDING.x * 2.0 - 4.0).max(200.0);
            ui.set_min_width(inner_w);
            ui.set_max_width(inner_w);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&series.title)
                        .strong()
                        .size(13.0)
                        .color(Theme::text_primary()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(StatePill::new(product_badge, accent));
                    ui.label(
                        egui::RichText::new(series.unit.format_value(latest_v))
                            .strong()
                            .monospace()
                            .size(13.0)
                            .color(accent),
                    );
                });
            });

            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(format!(
                    "Min: {}   Avg: {}   Max: {}   ({} pts)",
                    series.unit.format_value(min_v),
                    series.unit.format_value(avg_v),
                    series.unit.format_value(max_v),
                    series.points.len(),
                ))
                .size(10.0)
                .monospace()
                .color(Theme::text_muted()),
            );

            ui.add_space(6.0);

            let chart_h = 135.0;
            let (full_rect, resp) =
                ui.allocate_exact_size(Vec2::new(inner_w, chart_h), egui::Sense::hover());

            if ui.is_rect_visible(full_rect) && !series.points.is_empty() {
                let left_margin = 62.0;
                let bottom_margin = 16.0;
                let top_margin = 6.0;
                let plot = Rect::from_min_max(
                    Pos2::new(full_rect.left() + left_margin, full_rect.top() + top_margin),
                    Pos2::new(full_rect.right() - 6.0, full_rect.bottom() - bottom_margin),
                );

                // Grid lines & Y labels
                let grid_stroke = Stroke::new(1.0_f32, Theme::border().linear_multiply(0.55));
                let span = if (max_v - min_v).abs() < f64::EPSILON {
                    let pad = if max_v.abs() > 1.0 {
                        max_v.abs() * 0.1
                    } else {
                        1.0
                    };
                    (min_v - pad, max_v + pad)
                } else {
                    (min_v, max_v)
                };
                let (lo, hi) = span;
                let range = (hi - lo).max(1e-9);

                for frac in [0.0_f32, 0.5_f32, 1.0_f32] {
                    let y = plot.bottom() - plot.height() * frac;
                    ui.painter().line_segment(
                        [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                        grid_stroke,
                    );
                    let val_at = lo + range * frac as f64;
                    ui.painter().text(
                        Pos2::new(plot.left() - 4.0, y),
                        egui::Align2::RIGHT_CENTER,
                        series.unit.format_value(val_at),
                        egui::FontId::monospace(9.0),
                        Theme::text_muted(),
                    );
                }

                // Plot points
                let count = series.points.len();
                let coords: Vec<Pos2> = series
                    .points
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let x = if count == 1 {
                            plot.center().x
                        } else {
                            plot.left() + plot.width() * (i as f32 / (count - 1) as f32)
                        };
                        let y = plot.bottom() - plot.height() * ((p.value - lo) / range) as f32;
                        Pos2::new(x, y)
                    })
                    .collect();

                if coords.len() >= 2 {
                    let mut fill_poly = coords.clone();
                    fill_poly.push(Pos2::new(coords.last().unwrap().x, plot.bottom()));
                    fill_poly.push(Pos2::new(coords.first().unwrap().x, plot.bottom()));
                    ui.painter().add(Shape::convex_polygon(
                        fill_poly,
                        accent.linear_multiply(0.16),
                        Stroke::NONE,
                    ));
                    ui.painter()
                        .add(Shape::line(coords.clone(), Stroke::new(1.8_f32, accent)));
                }
                for pt in &coords {
                    ui.painter().circle_filled(*pt, 2.2, accent);
                }

                // X-axis start & end timestamps
                if let Some(first) = series.points.first() {
                    ui.painter().text(
                        Pos2::new(plot.left(), full_rect.bottom()),
                        egui::Align2::LEFT_BOTTOM,
                        &first.label,
                        egui::FontId::monospace(9.0),
                        Theme::text_muted(),
                    );
                }
                if count > 1
                    && let Some(last) = series.points.last()
                {
                    ui.painter().text(
                        Pos2::new(plot.right(), full_rect.bottom()),
                        egui::Align2::RIGHT_BOTTOM,
                        &last.label,
                        egui::FontId::monospace(9.0),
                        Theme::text_muted(),
                    );
                }

                // Hover crosshair + tooltip
                if resp.hovered()
                    && let Some(pointer) = ui.ctx().pointer_hover_pos()
                    && plot.contains(pointer)
                {
                    let nearest_idx = coords
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            (a.x - pointer.x).abs().total_cmp(&(b.x - pointer.x).abs())
                        })
                        .map(|(i, _)| i);
                    if let Some(idx) = nearest_idx {
                        let pt_pos = coords[idx];
                        let pt = &series.points[idx];
                        ui.painter().line_segment(
                            [
                                Pos2::new(pt_pos.x, plot.top()),
                                Pos2::new(pt_pos.x, plot.bottom()),
                            ],
                            Stroke::new(1.0_f32, Theme::text_muted()),
                        );
                        ui.painter().circle_filled(pt_pos, 4.5, accent);
                        resp.on_hover_text(format!(
                            "{}\n{} — {}",
                            series.title,
                            pt.label,
                            series.unit.format_value(pt.value)
                        ));
                    }
                }
            }
        });

    draw_hud_brackets(
        ui.painter(),
        frame_resp.response.rect,
        accent.linear_multiply(0.35),
        8.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_product_specific_index_patterns() {
        let raw = "logs-elasticsearch.*,logs-kibana.*";
        assert_eq!(
            resolve_telemetry_index_pattern(raw, TelemetryProductScope::Both),
            (raw.to_string(), None)
        );
        assert_eq!(
            resolve_telemetry_index_pattern(raw, TelemetryProductScope::Elasticsearch),
            ("logs-elasticsearch.*".to_string(), Some("elasticsearch"))
        );
        assert_eq!(
            resolve_telemetry_index_pattern(raw, TelemetryProductScope::Kibana),
            ("logs-kibana.*".to_string(), Some("kibana"))
        );

        let generic = "telemetry-*";
        assert_eq!(
            resolve_telemetry_index_pattern(generic, TelemetryProductScope::Kibana),
            ("telemetry-*".to_string(), Some("kibana"))
        );
    }

    #[test]
    fn builds_log_query_with_scope_level_and_kibana_filters() {
        let cfg = ClusterConfig {
            name: "prod".into(),
            logs_index_pattern: "logs-elasticsearch.*,logs-kibana.*".into(),
            ..Default::default()
        };
        let mut state = ClusterLogViewerState {
            product_scope: TelemetryProductScope::Kibana,
            level_filter: LogLevelFilter::Error,
            search_query: "timeout".into(),
            ..Default::default()
        };
        state.add_filter(KibanaFieldFilter::include_value("host.name", "kb-01"));
        state.add_filter(KibanaFieldFilter::exclude_value("event.action", "ping"));
        state.add_filter(KibanaFieldFilter::include_exists("error.message"));
        state.add_filter(KibanaFieldFilter::exclude_exists("debug.trace"));

        let (path, body) = build_dashboard_logs_query(&cfg, &state);
        assert_eq!(path, "/logs-kibana.*/_search");

        let body_str = body.to_string();
        assert!(body_str.contains("\"query_string\""));
        assert!(body_str.contains("timeout"));
        assert!(body_str.contains("\"host.name\":\"kb-01\""));
        assert!(body_str.contains("\"event.action\":\"ping\""));
        assert!(body_str.contains("\"field\":\"error.message\""));
        assert!(body_str.contains("\"field\":\"debug.trace\""));
        assert!(body_str.contains("\"log.level\""));
    }

    #[test]
    fn parses_logs_and_applies_kibana_filters_and_sorting() {
        let resp = json!({
            "hits": {
                "hits": [
                    {
                        "_id": "1",
                        "_index": "logs-elasticsearch-default",
                        "_source": {
                            "@timestamp": "2026-11-20T10:00:00Z",
                            "log": { "level": "info", "logger": "org.es.Node" },
                            "host": { "name": "es-node-1" },
                            "service": { "type": "elasticsearch" },
                            "message": "cluster started"
                        }
                    },
                    {
                        "_id": "2",
                        "_index": "logs-kibana-default",
                        "_source": {
                            "@timestamp": "2026-11-20T10:05:00Z",
                            "log": { "level": "error", "logger": "plugins.savedObjects" },
                            "host": { "name": "kb-node-1" },
                            "service": { "type": "kibana" },
                            "error": { "message": "boom" },
                            "message": "request failed"
                        }
                    }
                ]
            }
        });

        let rows = parse_dashboard_logs_response(&resp);
        assert_eq!(rows.len(), 2);

        let mut state = ClusterLogViewerState {
            rows,
            sort_col: LogSortColumn::Timestamp,
            sort_order: SortOrder::Asc,
            ..Default::default()
        };
        assert_eq!(visible_log_rows(&state).len(), 2);
        assert_eq!(visible_log_rows(&state)[0].host, "es-node-1");

        // Sort descending puts kb-node-1 first
        state.sort_order = SortOrder::Desc;
        assert_eq!(visible_log_rows(&state)[0].host, "kb-node-1");

        // Include exists
        state.add_filter(KibanaFieldFilter::include_exists("error.message"));
        let filtered = visible_log_rows(&state);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].service, "kibana");

        // Exclude value clears it
        state.add_filter(KibanaFieldFilter::exclude_value("host.name", "kb-node-1"));
        assert!(visible_log_rows(&state).is_empty());

        // Reset & test ExcludeExists
        state.filters.clear();
        state.add_filter(KibanaFieldFilter::exclude_exists("error.message"));
        let filtered = visible_log_rows(&state);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].service, "elasticsearch");
    }

    #[test]
    fn parses_metrics_aggregations_and_hits() {
        let resp = json!({
            "hits": {
                "hits": [
                    {
                        "_index": "metrics-kibana-default",
                        "_source": {
                            "@timestamp": "2026-11-20T10:00:00Z",
                            "kibana": {
                                "stats": {
                                    "response_times": { "avg_in_ms": 12.5 }
                                }
                            }
                        }
                    },
                    {
                        "_index": "metrics-kibana-default",
                        "_source": {
                            "@timestamp": "2026-11-20T10:01:00Z",
                            "kibana": {
                                "stats": {
                                    "response_times": { "avg_in_ms": 18.0 }
                                }
                            }
                        }
                    }
                ]
            },
            "aggregations": {
                "by_time": {
                    "buckets": [
                        {
                            "key": 1700000000000_i64,
                            "key_as_string": "2026-11-20T10:00:00.000Z",
                            "es_cpu_pct__0": { "value": 34.5 },
                            "es_jvm_heap_pct__0": { "value": 61.0 }
                        },
                        {
                            "key": 1700000060000_i64,
                            "key_as_string": "2026-11-20T10:01:00.000Z",
                            "es_cpu_pct__0": { "value": 41.0 },
                            "es_jvm_heap_pct__0": { "value": 64.0 }
                        }
                    ]
                }
            }
        });

        let parsed = parse_dashboard_metrics_response(&resp);
        assert_eq!(parsed.total_docs, 2);
        let cpu = parsed.series.iter().find(|s| s.id == "es_cpu_pct").unwrap();
        assert_eq!(cpu.points.len(), 2);
        assert_eq!(cpu.points[1].value, 41.0);

        let kb_rt = parsed
            .series
            .iter()
            .find(|s| s.id == "kb_resp_avg")
            .unwrap();
        assert_eq!(kb_rt.points.len(), 2);
        assert_eq!(kb_rt.points[0].value, 12.5);
    }
}
