//! God-mode Console: the Elastic Console, fanned out across many clusters and a loop of values.
//!
//! One request template is expanded into a list of [`GodJob`]s (one per selected cluster per
//! loop item) which `DrasticSmurfApp` executes sequentially on a background task. Each finished
//! job is appended to a combined, labelled response log.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Ui;

use crate::core::config::GodSavedQuery;
use crate::modules::console::{
    CONSOLE_METHODS, interpolate_variables, method_color, prettify_json_body, render_preset_list,
};
use crate::ui::theme::Theme;

/// Placeholder replaced by each loop value.
pub const LOOP_PLACEHOLDER: &str = "{{loop_item}}";
/// Placeholder replaced by the target cluster's name.
pub const CLUSTER_PLACEHOLDER: &str = "{{cluster_name}}";
/// Placeholder replaced by each value in the Extracted values list.
pub const RESULTS_PLACEHOLDER: &str = "{{results_item}}";

/// Per-job display cap, matching the single-cluster Console.
const JOB_DISPLAY_LIMIT: usize = 100_000;
/// Total display cap for the combined log; the full text is still available via "Copy all".
const TOTAL_DISPLAY_LIMIT: usize = 400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GodTab {
    #[default]
    Targets,
    Presets,
    History,
    Saved,
}

/// One concrete request: a fully interpolated call against one cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GodJob {
    pub cluster: String,
    pub item: Option<String>,
    pub result_item: Option<String>,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub use_kibana: bool,
}

/// A batch of jobs handed to the app for sequential execution.
pub struct GodRunRequest {
    pub run_id: u64,
    pub jobs: Vec<GodJob>,
    pub stop_on_error: bool,
    pub cancel: Arc<AtomicBool>,
}

/// Outcome of one executed job, produced by the background runner.
#[derive(Debug, Clone)]
pub struct GodJobResult {
    pub job: GodJob,
    pub ok: bool,
    /// Pretty-printed response body, or the error message.
    pub text: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct GodHistoryEntry {
    pub method: String,
    pub path: String,
    pub body: String,
    pub use_kibana: bool,
    pub clusters: Vec<String>,
    pub loop_text: String,
    pub extract_path: String,
}

/// Side effects requested by the UI, handled by the app after rendering.
#[derive(Default)]
pub struct GodConsoleActions {
    pub send: Option<GodRunRequest>,
    pub save_query: Option<GodSavedQuery>,
    pub delete_query: Option<String>,
}

#[derive(Debug, Default)]
pub struct GodConsoleState {
    // Request template
    pub method: String,
    pub path: String,
    pub body: String,
    pub use_kibana_host: bool,
    pub selected_clusters: Vec<String>,
    pub loop_text: String,
    /// Dotted JSON path pulled out of every response (e.g. `toplevel.setting.ilm`).
    pub extract_path: String,
    /// One extracted value per line; editable, and fed to `{{results_item}}`.
    pub extracted_text: String,
    /// Warnings from the last extracting run (missing path, object/array match, failed request).
    pub extract_warnings: Vec<String>,

    // Workspace
    pub active_tab: GodTab,
    pub preset_filter: String,
    pub target_filter: String,
    pub history: Vec<GodHistoryEntry>,
    pub history_index: Option<usize>,
    pub saved_queries: Vec<GodSavedQuery>,
    pub query_name_input: String,
    pub show_save_dialog: bool,
    pub body_height: Option<f32>,
    pub json_error: Option<String>,
    pub stop_on_error: bool,

    // Run state
    pub run_id: u64,
    pub is_running: bool,
    pub stopping: bool,
    pub total_jobs: usize,
    pub ok_count: usize,
    pub err_count: usize,
    pub cancel: Option<Arc<AtomicBool>>,
    /// Extraction parsed when the current run started; `None` when not extracting.
    run_extraction: Option<Extraction>,
    pub pending_confirm: Option<Vec<GodJob>>,
    /// Combined, possibly truncated log shown in the response box.
    pub response: String,
    /// Untruncated combined log for "Copy all".
    pub full_response: String,
    pub display_truncated: bool,
}

impl GodConsoleState {
    pub fn new() -> Self {
        Self {
            method: "GET".to_string(),
            path: "/_cluster/health".to_string(),
            body_height: Some(200.0),
            ..Default::default()
        }
    }

    fn is_selected(&self, cluster: &str) -> bool {
        self.selected_clusters.iter().any(|c| c == cluster)
    }

    fn set_selected(&mut self, cluster: &str, on: bool) {
        let present = self.is_selected(cluster);
        if on && !present {
            self.selected_clusters.push(cluster.to_string());
        } else if !on && present {
            self.selected_clusters.retain(|c| c != cluster);
        }
    }

    /// Selected clusters that are currently visible, in sidebar order.
    pub fn active_targets(&self, clusters: &[String]) -> Vec<String> {
        clusters
            .iter()
            .filter(|c| self.is_selected(c))
            .cloned()
            .collect()
    }

    fn clear_output(&mut self) {
        self.response.clear();
        self.full_response.clear();
        self.display_truncated = false;
        self.ok_count = 0;
        self.err_count = 0;
        self.total_jobs = 0;
    }

    fn append_text(&mut self, text: &str) {
        self.full_response.push_str(text);
        if self.display_truncated {
            return;
        }
        if self.response.len() + text.len() > TOTAL_DISPLAY_LIMIT {
            let room = TOTAL_DISPLAY_LIMIT.saturating_sub(self.response.len());
            self.response
                .push_str(truncate_at_char_boundary(text, room));
            self.response.push_str(
                "\n\n... [Output truncated for performance. Use 'Copy all' to get every result] ...\n",
            );
            self.display_truncated = true;
        } else {
            self.response.push_str(text);
        }
    }

    /// Record a finished job. Results from a superseded run are ignored.
    pub fn push_result(&mut self, run_id: u64, result: GodJobResult) {
        if run_id != self.run_id {
            return;
        }
        if result.ok {
            self.ok_count += 1;
        } else {
            self.err_count += 1;
        }
        let header = format_job_header(&result.job, result.ok, result.elapsed_ms);
        let extract_line = self.extract_for_result(&result);
        let mut body = result.text;
        if body.len() > JOB_DISPLAY_LIMIT {
            let cut = truncate_at_char_boundary(&body, JOB_DISPLAY_LIMIT).len();
            body.truncate(cut);
            body.push_str("\n... [Response truncated. Use 'Copy all' for the full payload] ...");
        }
        let block = match extract_line {
            Some(line) => format!("{}\n{}\n{}\n\n", header, line, body),
            None => format!("{}\n{}\n\n", header, body),
        };
        self.append_text(&block);
    }

    /// When the current run is extracting, pull row(s) out of this result, append them to
    /// the Extracted values list, and return the line(s) to show under the job header.
    fn extract_for_result(&mut self, result: &GodJobResult) -> Option<String> {
        let outcome = {
            let ex = self.run_extraction.as_ref()?;
            if result.ok {
                ex.extract(&result.text)
            } else {
                ExtractOutcome::failed(ex.columns(), "request failed")
            }
        };
        if !self.extracted_text.is_empty() && !self.extracted_text.ends_with('\n') {
            self.extracted_text.push('\n');
        }
        for row in &outcome.rows {
            self.extracted_text.push_str(row);
            self.extracted_text.push('\n');
        }
        let label = format!("[{}]{}", result.job.cluster, job_labels(&result.job));
        for w in &outcome.warnings {
            self.extract_warnings.push(format!("{}: {}", label, w));
        }
        Some(if outcome.rows.len() == 1 {
            let row = &outcome.rows[0];
            if outcome.warnings.is_empty() {
                format!("🔎 Extracted: {}", row)
            } else {
                format!("⚠️ Extraction: {} → {}", outcome.warnings.join("; "), row)
            }
        } else if outcome.warnings.is_empty() {
            format!("🔎 Extracted {} row(s)", outcome.rows.len())
        } else {
            let mut text = format!(
                "⚠️ Extracted {} row(s), {} warning(s)",
                outcome.rows.len(),
                outcome.warnings.len()
            );
            for w in outcome.warnings.iter().take(LOG_WARNING_LINES) {
                text.push_str(&format!("\n   ⚠️ {}", w));
            }
            if outcome.warnings.len() > LOG_WARNING_LINES {
                text.push_str(&format!(
                    "\n   … {} more (hover the warning count under Extracted values)",
                    outcome.warnings.len() - LOG_WARNING_LINES
                ));
            }
            text
        })
    }

    /// Mark a run finished. Results from a superseded run are ignored.
    pub fn finish(&mut self, run_id: u64) {
        if run_id != self.run_id {
            return;
        }
        let done = self.ok_count + self.err_count;
        if done < self.total_jobs {
            let reason = if self.stopping {
                "stopped by user"
            } else {
                "stopped after an error"
            };
            let text = format!(
                "⏹ Run {} after {}/{} requests.\n",
                reason, done, self.total_jobs
            );
            self.append_text(&text);
        }
        self.is_running = false;
        self.stopping = false;
        self.cancel = None;
    }

    fn start_run(&mut self, jobs: Vec<GodJob>, actions: &mut GodConsoleActions) {
        self.clear_output();
        self.run_id += 1;
        self.total_jobs = jobs.len();
        self.is_running = true;
        self.stopping = false;
        // One level of chaining only: a request that consumes {{results_item}} never
        // overwrites the list it is reading from.
        self.run_extraction = None;
        let expr = self.extract_path.trim();
        if !expr.is_empty() && !uses_results(&self.path, &self.body) {
            match parse_extraction(expr) {
                Ok(ex) => {
                    self.extracted_text.clear();
                    self.extract_warnings.clear();
                    self.run_extraction = Some(ex);
                }
                Err(e) => {
                    self.append_text(&format!("⚠️ Extraction disabled for this run: {}\n\n", e));
                }
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        actions.send = Some(GodRunRequest {
            run_id: self.run_id,
            jobs,
            stop_on_error: self.stop_on_error,
            cancel,
        });
    }

    fn record_history(&mut self, clusters: Vec<String>) {
        self.history.push(GodHistoryEntry {
            method: self.method.clone(),
            path: self.path.clone(),
            body: self.body.clone(),
            use_kibana: self.use_kibana_host,
            clusters,
            loop_text: self.loop_text.clone(),
            extract_path: self.extract_path.clone(),
        });
        self.history_index = Some(self.history.len() - 1);
    }

    fn load_history(&mut self, idx: usize) {
        if let Some(entry) = self.history.get(idx).cloned() {
            self.method = entry.method;
            self.path = entry.path;
            self.body = entry.body;
            self.use_kibana_host = entry.use_kibana;
            self.selected_clusters = entry.clusters;
            self.loop_text = entry.loop_text;
            self.extract_path = entry.extract_path;
            self.history_index = Some(idx);
        }
    }
}

fn truncate_at_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Parse a free-form list of loop values.
///
/// Accepts newline- and/or comma-separated values, optionally wrapped in a surrounding
/// `(...)` or `[...]`, with each value optionally wrapped in single or double quotes.
/// So `('a','b','c')`, `["a", "b"]` and one-value-per-line all work. Empty values are dropped.
pub fn parse_loop_items(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    let inner = if (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    inner
        .split(['\n', ','])
        .map(|raw| {
            let v = raw.trim();
            let unquoted = if v.len() >= 2
                && ((v.starts_with('\'') && v.ends_with('\''))
                    || (v.starts_with('"') && v.ends_with('"')))
            {
                &v[1..v.len() - 1]
            } else {
                v
            };
            unquoted.trim().to_string()
        })
        .filter(|v| !v.is_empty())
        .collect()
}

/// Parse the Extracted values list: strictly one value per line (values may contain commas).
pub fn parse_extracted_items(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
}

/// Markers written to the Extracted values list when a response yields no usable value.
const EXTRACT_MARKERS: &[&str] = &["null", "object", "array"];

/// Separator between paths in an extraction expression, and between fields in each
/// Extracted values row. Reserved: extracted values containing it are rewritten to `& &`.
pub const FIELD_SEPARATOR: &str = "&&";

/// Extraction warnings written to the log per response before summarising the rest.
const LOG_WARNING_LINES: usize = 5;

/// Runs with more requests than this ask for confirmation even for read-only methods.
pub const LARGE_RUN_CONFIRM: usize = 200;

/// Split one Extracted values row into its fields.
pub fn split_row(row: &str) -> Vec<&str> {
    row.split(FIELD_SEPARATOR).map(str::trim).collect()
}

/// Whether any field of a row is a `null` / `object` / `array` marker.
pub fn row_has_marker(row: &str) -> bool {
    split_row(row).iter().any(|f| EXTRACT_MARKERS.contains(f))
}

/// Normalise a path (`$.` prefix, `[n]` indexing) and split it into segments.
/// An empty path (or `$`) has no segments and refers to the whole response.
fn path_segments(path: &str) -> Vec<String> {
    let path = path.trim();
    let path = path
        .strip_prefix("$.")
        .or_else(|| path.strip_prefix('$'))
        .unwrap_or(path);
    if path.is_empty() {
        return Vec::new();
    }
    path.replace('[', ".")
        .replace(']', "")
        .split('.')
        .map(str::to_string)
        .collect()
}

/// Look up a dotted path in a JSON value.
///
/// Numeric segments index arrays (`hits.hits.0._id`, or `hits.hits[0]._id`). Object keys
/// that themselves contain dots (index names like `.ds-logs-2026.11.03`) are matched by
/// trying progressively longer joins of the remaining segments.
#[cfg(test)]
pub fn lookup_json_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    walk_json(value, &path_segments(path))
}

fn walk_json<'a, S: AsRef<str>>(
    value: &'a serde_json::Value,
    segs: &[S],
) -> Option<&'a serde_json::Value> {
    if segs.is_empty() {
        return Some(value);
    }
    match value {
        serde_json::Value::Object(map) => (1..=segs.len()).find_map(|end| {
            let key = segs[..end]
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(".");
            map.get(&key)
                .and_then(|child| walk_json(child, &segs[end..]))
        }),
        serde_json::Value::Array(arr) => {
            let idx: usize = segs[0].as_ref().parse().ok()?;
            walk_json(arr.get(idx)?, &segs[1..])
        }
        _ => None,
    }
}

/// Make an extracted string safe for a one-line, `&&`-separated row.
fn clean_field(v: &str) -> String {
    v.replace(['\r', '\n'], " ").replace(FIELD_SEPARATOR, "& &")
}

/// Turn a looked-up JSON value into one row field, plus a warning for unusable values.
fn to_field(value: Option<&serde_json::Value>, what: &str) -> (String, Option<String>) {
    use serde_json::Value;
    match value {
        None => ("null".to_string(), Some(format!("'{}' not found", what))),
        Some(Value::Null) => ("null".to_string(), Some(format!("'{}' is null", what))),
        Some(Value::Object(_)) => (
            "object".to_string(),
            Some(format!("'{}' matched an object, not a value", what)),
        ),
        Some(Value::Array(_)) => (
            "array".to_string(),
            Some(format!("'{}' matched an array, not a value", what)),
        ),
        Some(Value::String(v)) => (clean_field(v), None),
        Some(other) => (other.to_string(), None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtractPart {
    raw: String,
    segments: Vec<String>,
    /// Position of the `*` segment, if this part uses the wildcard.
    star: Option<usize>,
}

/// A parsed extraction expression: one or more `&&`-joined paths with at most one `*`
/// position, shared by every path that uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extraction {
    parts: Vec<ExtractPart>,
    /// Segments before the `*`, when any part uses the wildcard.
    prefix: Option<Vec<String>>,
}

/// Rows and warnings extracted from one response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractOutcome {
    pub rows: Vec<String>,
    pub warnings: Vec<String>,
}

impl ExtractOutcome {
    /// A single all-`null` row with one warning.
    pub fn failed(columns: usize, warning: impl Into<String>) -> Self {
        Self {
            rows: vec![vec!["null"; columns.max(1)].join(" && ")],
            warnings: vec![warning.into()],
        }
    }
}

/// Parse an extraction expression such as `toplevel.setting.ilm` or
/// `indices.*&&indices.*.policy`.
///
/// Rules: paths are joined with `&&`; `*` must be a whole segment; there is at most one `*`
/// per path, and every path that uses it must share the same prefix before it (so all
/// wildcard columns walk the same object/array and line up row by row).
pub fn parse_extraction(expr: &str) -> Result<Extraction, String> {
    let mut parts = Vec::new();
    let mut prefix: Option<Vec<String>> = None;
    for raw in expr.split(FIELD_SEPARATOR) {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty path next to &&".to_string());
        }
        let segments = path_segments(raw);
        let stars: Vec<usize> = segments
            .iter()
            .enumerate()
            .filter(|(_, seg)| seg.contains('*'))
            .map(|(i, _)| i)
            .collect();
        if stars.len() > 1 {
            return Err(format!("'{}' has more than one *", raw));
        }
        let star = stars.first().copied();
        if let Some(i) = star {
            if segments[i] != "*" {
                return Err(format!(
                    "'{}': * must be a whole path segment, e.g. indices.*.policy",
                    raw
                ));
            }
            let p = segments[..i].to_vec();
            match &prefix {
                Some(existing) if *existing != p => {
                    return Err(format!(
                        "every * must sit at the same path: '{}' vs '{}'",
                        star_path(existing),
                        star_path(&p)
                    ));
                }
                _ => prefix = Some(p),
            }
        }
        parts.push(ExtractPart {
            raw: raw.to_string(),
            segments,
            star,
        });
    }
    Ok(Extraction { parts, prefix })
}

/// Display form of a wildcard position, e.g. `indices.*` (or `*` at the top level).
fn star_path(prefix: &[String]) -> String {
    if prefix.is_empty() {
        "*".to_string()
    } else {
        format!("{}.*", prefix.join("."))
    }
}

impl Extraction {
    /// Number of fields in every extracted row.
    pub fn columns(&self) -> usize {
        self.parts.len()
    }

    /// Extract rows from one response body.
    ///
    /// Without a wildcard this yields exactly one row. With one, it yields a row per key
    /// (object) or element (array) at the wildcard; a path ending in `*` gives that key or
    /// index. Paths without a wildcard are evaluated once and repeated on every row.
    /// Unusable values become `null` / `object` / `array` markers with a warning.
    pub fn extract(&self, text: &str) -> ExtractOutcome {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            return ExtractOutcome::failed(self.columns(), "response is not JSON");
        };
        let mut warnings = Vec::new();
        // Fixed (non-wildcard) columns: evaluated once, warned once.
        let fixed: Vec<Option<String>> = self
            .parts
            .iter()
            .map(|part| {
                part.star.is_none().then(|| {
                    let (v, w) = to_field(walk_json(&json, &part.segments), &part.raw);
                    warnings.extend(w);
                    v
                })
            })
            .collect();

        let Some(prefix) = &self.prefix else {
            let row = fixed.into_iter().flatten().collect::<Vec<_>>().join(" && ");
            return ExtractOutcome {
                rows: vec![row],
                warnings,
            };
        };

        let entries: Vec<(String, &serde_json::Value)> = match walk_json(&json, prefix) {
            Some(serde_json::Value::Object(map)) => {
                map.iter().map(|(k, v)| (k.clone(), v)).collect()
            }
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
            Some(_) => {
                return ExtractOutcome::failed(
                    self.columns(),
                    format!("'{}' is not an object or array", star_path(prefix)),
                );
            }
            None => {
                return ExtractOutcome::failed(
                    self.columns(),
                    format!("'{}' not found", prefix.join(".")),
                );
            }
        };
        if entries.is_empty() {
            return ExtractOutcome::failed(
                self.columns(),
                format!("'{}' matched nothing", star_path(prefix)),
            );
        }

        let rows = entries
            .iter()
            .map(|(key, child)| {
                self.parts
                    .iter()
                    .zip(&fixed)
                    .map(|(part, fixed)| match (fixed, part.star) {
                        (Some(v), _) => v.clone(),
                        (None, Some(star)) => {
                            let suffix = &part.segments[star + 1..];
                            if suffix.is_empty() {
                                clean_field(key)
                            } else {
                                let what = format!("{} @ {}", part.raw, key);
                                let (v, w) = to_field(walk_json(child, suffix), &what);
                                warnings.extend(w);
                                v
                            }
                        }
                        (None, None) => unreachable!("non-wildcard parts are always fixed"),
                    })
                    .collect::<Vec<_>>()
                    .join(" && ")
            })
            .collect();
        ExtractOutcome { rows, warnings }
    }
}

/// Highest `{{results_item…}}` field referenced in a template: 0 when unused, 1 for the bare
/// `{{results_item}}`, N for `{{results_item.N}}`.
pub fn max_results_field(text: &str) -> usize {
    let mut max = 0;
    let mut rest = text;
    while let Some(pos) = rest.find(RESULTS_PREFIX) {
        let after = &rest[pos + RESULTS_PREFIX.len()..];
        if let Some((n, _)) = parse_results_ref(after) {
            max = max.max(n);
        }
        rest = after;
    }
    max
}

/// `{{results_item` — shared prefix of the bare and numbered results placeholders.
const RESULTS_PREFIX: &str = "{{results_item";

/// Parse what follows `{{results_item`: `}}` is field 1, `.N}}` is field N.
/// Returns the field number and the number of bytes consumed.
fn parse_results_ref(after: &str) -> Option<(usize, usize)> {
    if after.starts_with("}}") {
        return Some((1, 2));
    }
    let digits_and_rest = after.strip_prefix('.')?;
    let digits: String = digits_and_rest
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() || !digits_and_rest[digits.len()..].starts_with("}}") {
        return None;
    }
    let n: usize = digits.parse().ok()?;
    (n >= 1).then_some((n, 1 + digits.len() + 2))
}

/// Replace `{{results_item}}` / `{{results_item.N}}` with a row's fields in a single pass,
/// so inserted response data is never itself re-substituted. References to fields the row
/// doesn't have are left as-is.
fn substitute_results(template: &str, row: &str) -> String {
    let fields = split_row(row);
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(pos) = rest.find(RESULTS_PREFIX) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + RESULTS_PREFIX.len()..];
        match parse_results_ref(after).and_then(|(n, used)| Some((fields.get(n - 1)?, used))) {
            Some((field, used)) => {
                out.push_str(field);
                rest = &after[used..];
            }
            None => {
                out.push_str(RESULTS_PREFIX);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether the request template references `{{results_item}}` or `{{results_item.N}}`.
pub fn uses_results(path: &str, body: &str) -> bool {
    max_results_field(path).max(max_results_field(body)) > 0
}

/// Whether the request template references the loop placeholder.
pub fn uses_loop(path: &str, body: &str) -> bool {
    path.contains(LOOP_PLACEHOLDER) || body.contains(LOOP_PLACEHOLDER)
}

/// Interpolate a template for one cluster, optional loop item and optional results item.
///
/// Order: `{{cluster_name}}` and `{{loop_item}}` first, then the cluster's own Console
/// variables (so a loop value may itself contain a `{{var}}` reference), then
/// `{{results_item}}` last, so response data is never re-interpreted as a template.
pub fn interpolate_for(
    template: &str,
    cluster: &str,
    item: Option<&str>,
    result_item: Option<&str>,
    cluster_vars: &[(String, String)],
) -> String {
    let mut out = template.replace(CLUSTER_PLACEHOLDER, cluster);
    if let Some(item) = item {
        out = out.replace(LOOP_PLACEHOLDER, item);
    }
    out = interpolate_variables(&out, cluster_vars);
    if let Some(row) = result_item {
        out = substitute_results(&out, row);
    }
    out
}

/// Expand a request template into concrete jobs: for each cluster (in order), each loop item,
/// each results item.
///
/// A list only multiplies the jobs when it is non-empty and its placeholder is used in the
/// path or body; with neither, each cluster gets exactly one job.
#[allow(clippy::too_many_arguments)]
pub fn build_jobs(
    clusters: &[String],
    items: &[String],
    results: &[String],
    method: &str,
    path: &str,
    body: &str,
    use_kibana: bool,
    vars_for: impl Fn(&str) -> Vec<(String, String)>,
) -> Vec<GodJob> {
    let loop_items: Vec<Option<&str>> = if !items.is_empty() && uses_loop(path, body) {
        items.iter().map(|i| Some(i.as_str())).collect()
    } else {
        vec![None]
    };
    let result_items: Vec<Option<&str>> = if !results.is_empty() && uses_results(path, body) {
        results.iter().map(|r| Some(r.as_str())).collect()
    } else {
        vec![None]
    };
    let mut jobs = Vec::new();
    for cluster in clusters {
        let vars = vars_for(cluster);
        for &item in &loop_items {
            for &result_item in &result_items {
                let interp_body = interpolate_for(body, cluster, item, result_item, &vars);
                jobs.push(GodJob {
                    cluster: cluster.clone(),
                    item: item.map(str::to_string),
                    result_item: result_item.map(str::to_string),
                    method: method.to_string(),
                    path: interpolate_for(path, cluster, item, result_item, &vars),
                    body: if interp_body.trim().is_empty() {
                        None
                    } else {
                        Some(interp_body)
                    },
                    use_kibana,
                });
            }
        }
    }
    jobs
}

/// ` loop_item=… results_item=…` labels for a job (empty when it uses neither).
fn job_labels(job: &GodJob) -> String {
    let mut out = String::new();
    if let Some(i) = &job.item {
        out.push_str(&format!(" loop_item={}", i));
    }
    if let Some(r) = &job.result_item {
        out.push_str(&format!(" results_item={}", r));
    }
    out
}

/// The one-line header written above each job's output in the combined log.
pub fn format_job_header(job: &GodJob, ok: bool, elapsed_ms: u128) -> String {
    let item = job_labels(job);
    let status = if ok { "✅ OK" } else { "❌ ERROR" };
    format!(
        "━━ [{}]{} ━━ {} {} → {} ({} ms)",
        job.cluster, item, job.method, job.path, status, elapsed_ms
    )
}

fn is_read_only_method(method: &str) -> bool {
    matches!(method, "GET" | "HEAD")
}

fn small_muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text.into())
            .color(Theme::text_muted())
            .size(11.0),
    );
}

#[allow(clippy::too_many_lines)]
pub fn render_god_console_module(
    ui: &mut Ui,
    state: &mut GodConsoleState,
    clusters: &[String],
    vars_for: &dyn Fn(&str) -> Vec<(String, String)>,
    actions: &mut GodConsoleActions,
) {
    ui.horizontal(|ui| {
        ui.heading("⚡ God-mode Console");
        small_muted(
            ui,
            "Run one request across many clusters, optionally looping over a list of values.",
        );
    });
    ui.add_space(8.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    let targets = state.active_targets(clusters);
    let items = parse_loop_items(&state.loop_text);
    let results = parse_extracted_items(&state.extracted_text);
    let looping = !items.is_empty() && uses_loop(&state.path, &state.body);
    let resulting = !results.is_empty() && uses_results(&state.path, &state.body);
    let per_cluster =
        if looping { items.len() } else { 1 } * if resulting { results.len() } else { 1 };
    let job_count = targets.len() * per_cluster;

    render_confirm_modal(ui, state, actions);

    let total_available = ui.available_size() - egui::vec2(0.0, 12.0);
    ui.allocate_ui_with_layout(
        total_available,
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            // --- LEFT COLUMN: Workspace ---
            let left_width = 300.0;
            ui.vertical(|ui| {
                ui.set_width(left_width);
                let height = ui.available_height();
                egui::Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.set_height(height - 32.0);
                        render_workspace(ui, state, clusters, &items, &results, actions);
                    });
            });

            ui.add_space(8.0);

            // --- RIGHT COLUMN: Editor & Response ---
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                let height = ui.available_height();
                egui::Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.set_height(height - 32.0);
                        render_editor(
                            ui, state, &targets, &items, &results, looping, resulting, job_count,
                            vars_for, actions,
                        );
                    });
            });
        },
    );
}

fn render_workspace(
    ui: &mut Ui,
    state: &mut GodConsoleState,
    clusters: &[String],
    items: &[String],
    results: &[String],
    actions: &mut GodConsoleActions,
) {
    ui.label(
        egui::RichText::new("WORKSPACE")
            .strong()
            .color(Theme::text_secondary())
            .size(11.0),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        for (tab, label) in [
            (GodTab::Targets, "🎯 Targets"),
            (GodTab::Presets, "📚 Presets"),
            (GodTab::History, "⏱️ Hist"),
            (GodTab::Saved, "💾 Saved"),
        ] {
            let is_active = state.active_tab == tab;
            let text = egui::RichText::new(label).size(11.0);
            let text = if is_active {
                let active_bg = Theme::accent().linear_multiply(0.25);
                text.color(Theme::contrast_text_color(active_bg)).strong()
            } else {
                text.color(Theme::text_secondary())
            };
            if ui.selectable_label(is_active, text).clicked() {
                state.active_tab = tab;
            }
        }
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(8.0);

    if state.active_tab == GodTab::Presets {
        ui.horizontal(|ui| {
            let clear_width = 24.0;
            ui.add(
                egui::TextEdit::singleline(&mut state.preset_filter)
                    .hint_text("🔍 Filter commands by title...")
                    .desired_width(
                        ui.available_width() - clear_width - ui.spacing().item_spacing.x,
                    ),
            );
            if ui
                .add_enabled(
                    !state.preset_filter.is_empty(),
                    egui::Button::new("✖").min_size(egui::vec2(clear_width, 0.0)),
                )
                .clicked()
            {
                state.preset_filter.clear();
            }
        });
        ui.add_space(6.0);
    }

    let scroll_height = (ui.available_height() - 4.0).max(100.0);
    egui::ScrollArea::vertical()
        .id_salt("god_console_workspace_scroll")
        .max_height(scroll_height)
        .show(ui, |ui| match state.active_tab {
            GodTab::Targets => render_targets_tab(ui, state, clusters, items, results),
            GodTab::Presets => {
                ui.horizontal(|ui| {
                    ui.hyperlink_to(
                        "📚 ES API Docs",
                        "https://www.elastic.co/docs/api/doc/elasticsearch/",
                    );
                    ui.label("|");
                    ui.hyperlink_to(
                        "🎨 Kibana API Docs",
                        "https://www.elastic.co/docs/api/doc/kibana/",
                    );
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                if let Some(preset) =
                    render_preset_list(ui, &state.preset_filter, "god_console_preset")
                {
                    state.method = preset.method.to_string();
                    state.path = preset.path.to_string();
                    state.body = preset.body.map(|b| b.to_string()).unwrap_or_default();
                    state.use_kibana_host = preset.use_kibana;
                    state.history_index = None;
                }
            }
            GodTab::History => render_history_tab(ui, state),
            GodTab::Saved => render_saved_tab(ui, state, actions),
        });
}

fn render_targets_tab(
    ui: &mut Ui,
    state: &mut GodConsoleState,
    clusters: &[String],
    items: &[String],
    results: &[String],
) {
    // --- Cluster checklist ---
    let visible: Vec<&String> = clusters
        .iter()
        .filter(|c| {
            let f = state.target_filter.trim().to_lowercase();
            f.is_empty() || c.to_lowercase().contains(&f)
        })
        .collect();
    let selected_count = state.active_targets(clusters).len();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Clusters").strong().size(13.0));
        small_muted(
            ui,
            format!("{}/{} selected", selected_count, clusters.len()),
        );
    });
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.target_filter)
            .hint_text("🔍 Filter clusters...")
            .desired_width(ui.available_width()),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui
            .small_button("Select all")
            .on_hover_text("Select every cluster matching the filter")
            .clicked()
        {
            for c in &visible {
                state.set_selected(c, true);
            }
        }
        if ui
            .small_button("Select none")
            .on_hover_text("Deselect every cluster matching the filter")
            .clicked()
        {
            for c in &visible {
                state.set_selected(c, false);
            }
        }
    });
    ui.add_space(4.0);
    for cluster in &visible {
        let mut on = state.is_selected(cluster);
        if ui.checkbox(&mut on, cluster.as_str()).changed() {
            state.set_selected(cluster, on);
        }
    }
    if visible.is_empty() {
        small_muted(ui, "No clusters match this filter.");
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- Loop values ---
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Loop values").strong().size(13.0));
        small_muted(ui, format!("{} value(s)", items.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !state.loop_text.is_empty(),
                    egui::Button::new("Clear").small(),
                )
                .clicked()
            {
                state.loop_text.clear();
            }
        });
    });
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::multiline(&mut state.loop_text)
            .hint_text("one per line, or 'a','b','c'")
            .code_editor()
            .desired_rows(6)
            .desired_width(ui.available_width()),
    );
    ui.add_space(4.0);
    small_muted(
        ui,
        format!(
            "Use {} in the path or body. {} and this cluster's Console variables are also substituted per target.",
            LOOP_PLACEHOLDER, CLUSTER_PLACEHOLDER
        ),
    );
    if !items.is_empty() {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for item in items.iter().take(50) {
                ui.label(
                    egui::RichText::new(item)
                        .monospace()
                        .size(11.0)
                        .background_color(Theme::bg_input()),
                );
            }
            if items.len() > 50 {
                small_muted(ui, format!("+{} more", items.len() - 50));
            }
        });
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    render_extraction_section(ui, state, results);
}

fn render_extraction_section(ui: &mut Ui, state: &mut GodConsoleState, results: &[String]) {
    // --- Result extraction path ---
    ui.label(egui::RichText::new("Result extraction").strong().size(13.0));
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.extract_path)
            .hint_text("e.g. toplevel.setting.ilm  or  *&&*.settings.index.lifecycle.name")
            .code_editor()
            .desired_width(ui.available_width()),
    );
    ui.add_space(2.0);
    let consuming = uses_results(&state.path, &state.body);
    if consuming && !state.extract_path.trim().is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "⏸ Paused: this request uses {}, so it won't overwrite the list it reads (one level of chaining).",
                RESULTS_PLACEHOLDER
            ))
            .color(Theme::text_secondary())
            .size(11.0),
        );
    } else if !state.extract_path.trim().is_empty() {
        match parse_extraction(&state.extract_path) {
            Ok(ex) if ex.columns() > 1 => {
                let names: Vec<String> = (1..=ex.columns())
                    .map(|i| {
                        if i == 1 {
                            RESULTS_PLACEHOLDER.to_string()
                        } else {
                            format!("{}.{}}}}}", RESULTS_PREFIX, i)
                        }
                    })
                    .collect();
                small_muted(
                    ui,
                    format!("{} columns → {}", ex.columns(), names.join(", ")),
                );
            }
            Ok(_) => {}
            Err(e) => {
                ui.label(
                    egui::RichText::new(format!("⚠️ {}", e))
                        .color(Theme::danger())
                        .size(11.0),
                );
            }
        }
    }
    ui.label(
        egui::RichText::new("ℹ Path syntax")
            .color(Theme::text_secondary())
            .size(11.0),
    )
    .on_hover_text(
        "Dotted JSON path pulled from every response on Send.\n\
             • Numeric segments index arrays: hits.hits.0._id or hits.hits[0]._id\n\
             • Keys starting with '.' get a doubled dot: indices..ds-logs-x.policy\n\
             • One * per expression matches every key/element; one row each.\n\
             \u{20}\u{20}A path ending in * gives the key itself: indices.*\n\
             • Join paths with && for more columns: indices.*&&indices.*.policy\n\
             \u{20}\u{20}All * paths must share the same prefix; others repeat on each row.\n\
             • Use {{results_item}} (column 1) and {{results_item.2}}, .3 … in the request.",
    );

    ui.add_space(8.0);

    // --- Extracted values ---
    let marker_count = results.iter().filter(|v| row_has_marker(v)).count();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Extracted values").strong().size(13.0));
        small_muted(ui, format!("{} value(s)", results.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let idle = !state.is_running;
            if ui
                .add_enabled(
                    idle && !state.extracted_text.is_empty(),
                    egui::Button::new("Clear").small(),
                )
                .clicked()
            {
                state.extracted_text.clear();
                state.extract_warnings.clear();
            }
            if ui
                .add_enabled(
                    idle && results.len() > 1,
                    egui::Button::new("Unique").small(),
                )
                .on_hover_text("Remove duplicate rows, keeping the first occurrence")
                .clicked()
            {
                let mut seen = std::collections::HashSet::new();
                let unique: Vec<&String> =
                    results.iter().filter(|v| seen.insert(v.as_str())).collect();
                state.extracted_text = unique
                    .iter()
                    .map(|v| format!("{}\n", v))
                    .collect::<String>();
            }
            if ui
                .add_enabled(
                    idle && marker_count > 0,
                    egui::Button::new("Drop ⚠").small(),
                )
                .on_hover_text("Remove rows with a null / object / array placeholder in any column")
                .clicked()
            {
                state.extracted_text = results
                    .iter()
                    .filter(|v| !row_has_marker(v))
                    .map(|v| format!("{}\n", v))
                    .collect::<String>();
            }
        });
    });
    ui.add_space(4.0);
    ui.add_enabled(
        !state.is_running,
        egui::TextEdit::multiline(&mut state.extracted_text)
            .hint_text("filled on Send, one row per line (columns joined by &&); editable")
            .code_editor()
            .desired_rows(6)
            .desired_width(ui.available_width()),
    );
    ui.add_space(4.0);
    small_muted(
        ui,
        format!(
            "Use {} in the path or body to run once per row.",
            RESULTS_PLACEHOLDER
        ),
    );
    let used = max_results_field(&state.path).max(max_results_field(&state.body));
    let min_fields = results.iter().map(|r| split_row(r).len()).min();
    if let Some(min_fields) = min_fields.filter(|&m| used > m) {
        ui.label(
            egui::RichText::new(format!(
                "⚠️ The request uses column {} but some rows only have {}; those stay literal.",
                used, min_fields
            ))
            .color(Theme::warning())
            .size(11.0),
        );
    }
    if !state.extract_warnings.is_empty() {
        ui.add_space(4.0);
        let n = state.extract_warnings.len();
        let mut hover = state
            .extract_warnings
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if n > 50 {
            hover.push_str(&format!("\n… {} more", n - 50));
        }
        ui.label(
            egui::RichText::new(format!("⚠️ {} extraction warning(s)", n))
                .color(Theme::warning())
                .size(11.0),
        )
        .on_hover_text(hover);
    }
    if marker_count > 0 {
        ui.label(
            egui::RichText::new(format!(
                "⚠️ {} row(s) contain null/object/array placeholders, which would be sent as literal text.",
                marker_count
            ))
            .color(Theme::warning())
            .size(11.0),
        );
    }
}

fn render_history_tab(ui: &mut Ui, state: &mut GodConsoleState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("History").strong().size(13.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("🗑 Clear").clicked() {
                state.history.clear();
                state.history_index = None;
            }
        });
    });
    ui.add_space(6.0);

    if state.history.is_empty() {
        small_muted(ui, "No request history.");
        return;
    }
    let mut load = None;
    for (idx, entry) in state.history.iter().enumerate().rev() {
        let loop_n = parse_loop_items(&entry.loop_text).len();
        let label = format!(
            "{} {}  ·  {}c{}",
            entry.method,
            entry.path,
            entry.clusters.len(),
            if loop_n > 0 {
                format!(" × {}", loop_n)
            } else {
                String::new()
            }
        );
        let is_loaded = state.history_index == Some(idx);
        let text = egui::RichText::new(label).size(12.0);
        let text = if is_loaded {
            text.color(Theme::accent()).strong()
        } else {
            text.color(Theme::text_primary())
        };
        let btn = ui.selectable_label(is_loaded, text);
        if btn.clicked() {
            load = Some(idx);
        }
        btn.on_hover_text(format!(
            "Clusters: {}\nLoop values: {}\nExtraction: {}\nBody: {}",
            entry.clusters.join(", "),
            entry.loop_text.replace('\n', ", "),
            entry.extract_path,
            entry.body
        ));
        ui.add_space(4.0);
    }
    if let Some(idx) = load {
        state.load_history(idx);
    }
}

fn render_saved_tab(ui: &mut Ui, state: &mut GodConsoleState, actions: &mut GodConsoleActions) {
    ui.label(egui::RichText::new("Saved Queries").strong().size(13.0));
    small_muted(ui, "Saved with their target clusters and loop values.");
    ui.add_space(6.0);

    if state.saved_queries.is_empty() {
        small_muted(ui, "No saved queries.");
        return;
    }
    let mut load = None;
    for (idx, query) in state.saved_queries.iter().enumerate() {
        ui.horizontal(|ui| {
            let btn = ui.button(&query.name);
            if btn.clicked() {
                load = Some(idx);
            }
            btn.on_hover_text(format!(
                "{} {}\nClusters: {}\nLoop values: {}\nExtraction: {}",
                query.method,
                query.path,
                query.clusters.join(", "),
                query.loop_items.replace('\n', ", "),
                query.extract_path
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🗑").on_hover_text("Delete Saved Query").clicked() {
                    actions.delete_query = Some(query.name.clone());
                }
            });
        });
        ui.add_space(4.0);
    }
    if let Some(idx) = load {
        let q = state.saved_queries[idx].clone();
        state.method = q.method;
        state.path = q.path;
        state.body = q.body.unwrap_or_default();
        state.use_kibana_host = q.use_kibana;
        state.loop_text = q.loop_items;
        state.extract_path = q.extract_path;
        if !q.clusters.is_empty() {
            state.selected_clusters = q.clusters;
        }
        state.history_index = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn render_editor(
    ui: &mut Ui,
    state: &mut GodConsoleState,
    targets: &[String],
    items: &[String],
    results: &[String],
    looping: bool,
    resulting: bool,
    job_count: usize,
    vars_for: &dyn Fn(&str) -> Vec<(String, String)>,
    actions: &mut GodConsoleActions,
) {
    // --- Method / host / target summary row ---
    ui.horizontal(|ui| {
        ui.label("Method:");
        egui::ComboBox::from_id_salt("god_console_method")
            .selected_text(
                egui::RichText::new(&state.method)
                    .color(method_color(&state.method))
                    .strong(),
            )
            .show_ui(ui, |ui| {
                for &m in CONSOLE_METHODS {
                    ui.selectable_value(
                        &mut state.method,
                        m.to_string(),
                        egui::RichText::new(m).color(method_color(m)).strong(),
                    );
                }
            });
        ui.add_space(8.0);
        ui.checkbox(&mut state.use_kibana_host, "Kibana Host");
        ui.add_space(8.0);
        ui.checkbox(&mut state.stop_on_error, "Stop on first error");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let summary = if targets.is_empty() {
                egui::RichText::new("🎯 No clusters selected").color(Theme::warning())
            } else {
                let mut text = format!("🎯 {} cluster(s)", targets.len());
                if looping {
                    text.push_str(&format!(" × {} item(s)", items.len()));
                }
                if resulting {
                    text.push_str(&format!(" × {} result(s)", results.len()));
                }
                text.push_str(&format!(" = {} request(s)", job_count));
                egui::RichText::new(text).color(Theme::accent())
            };
            let resp = ui.label(summary.strong());
            if !targets.is_empty() {
                resp.on_hover_text(targets.join("\n"));
            }
        });
    });

    ui.add_space(8.0);

    // --- Path row with actions ---
    let build = |state: &GodConsoleState| {
        build_jobs(
            targets,
            items,
            results,
            &state.method,
            &state.path,
            &state.body,
            state.use_kibana_host,
            vars_for,
        )
    };

    ui.horizontal(|ui| {
        ui.label("Path:");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.is_running {
                let stop = ui.add_enabled(
                    !state.stopping,
                    egui::Button::new(
                        egui::RichText::new("⏹ Stop")
                            .color(Theme::contrast_text_color(Theme::danger())),
                    )
                    .fill(Theme::danger()),
                );
                if stop.clicked() {
                    if let Some(cancel) = &state.cancel {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    state.stopping = true;
                }
                ui.spinner();
                ui.label(
                    egui::RichText::new(format!(
                        "{}/{}",
                        state.ok_count + state.err_count,
                        state.total_jobs
                    ))
                    .strong(),
                );
            } else {
                let accent = Theme::accent();
                let send = ui
                    .add_enabled(
                        job_count > 0,
                        egui::Button::new(
                            egui::RichText::new("⚡ Send")
                                .color(Theme::contrast_text_color(accent)),
                        )
                        .fill(accent),
                    )
                    .on_hover_text("Run against every selected cluster, in order");
                if send.clicked() {
                    let jobs = build(state);
                    state.record_history(targets.to_vec());
                    if (!is_read_only_method(&state.method) && jobs.len() > 1)
                        || jobs.len() > LARGE_RUN_CONFIRM
                    {
                        state.pending_confirm = Some(jobs);
                    } else {
                        state.start_run(jobs, actions);
                    }
                }

                let dry = ui
                    .add_enabled(job_count > 0, egui::Button::new("🧪 Dry run"))
                    .on_hover_text("Show every request that would be sent, without sending");
                if dry.clicked() {
                    let jobs = build(state);
                    state.clear_output();
                    let mut text =
                        format!("🧪 Dry run: {} request(s), nothing sent.\n\n", jobs.len());
                    for job in &jobs {
                        let item = job_labels(job);
                        text.push_str(&format!(
                            "[{}]{}  {} {}{}\n",
                            job.cluster,
                            item,
                            job.method,
                            job.path,
                            if job.use_kibana { "  (Kibana)" } else { "" }
                        ));
                        if let Some(b) = &job.body {
                            text.push_str(b);
                            text.push('\n');
                        }
                    }
                    state.append_text(&text);
                }
            }

            if ui.button("💾 Save").clicked() {
                state.show_save_dialog = true;
                state.query_name_input.clear();
            }

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.path)
                        .desired_width((ui.available_width() - 8.0).max(100.0)),
                );
            });
        });
    });

    // Evaluated preview for the first job, plus a warning when loop values go unused.
    if let Some(first) = targets.first() {
        let item = if looping {
            items.first().map(String::as_str)
        } else {
            None
        };
        let result_item = if resulting {
            results.first().map(String::as_str)
        } else {
            None
        };
        let vars = vars_for(first);
        let evaluated = interpolate_for(&state.path, first, item, result_item, &vars);
        if evaluated != state.path {
            small_muted(
                ui,
                format!("📝 Evaluated path (first: {}): {}", first, evaluated),
            );
        }
    }
    if !items.is_empty() && !looping {
        ui.label(
            egui::RichText::new(format!(
                "⚠️ {} loop value(s) listed but {} isn't used in the path or body. Each cluster runs once.",
                items.len(),
                LOOP_PLACEHOLDER
            ))
            .color(Theme::warning())
            .size(11.0),
        );
    }
    if results.is_empty() && uses_results(&state.path, &state.body) {
        ui.label(
            egui::RichText::new(format!(
                "⚠️ {} is used but Extracted values is empty, so it will be sent literally.",
                RESULTS_PLACEHOLDER
            ))
            .color(Theme::warning())
            .size(11.0),
        );
    }
    ui.add_space(4.0);

    // Save dialog
    if state.show_save_dialog {
        ui.horizontal(|ui| {
            ui.label("Query Name:");
            ui.text_edit_singleline(&mut state.query_name_input);
            if ui.button("Save").clicked() && !state.query_name_input.trim().is_empty() {
                actions.save_query = Some(GodSavedQuery {
                    name: state.query_name_input.trim().to_string(),
                    method: state.method.clone(),
                    path: state.path.clone(),
                    body: if state.body.trim().is_empty() {
                        None
                    } else {
                        Some(state.body.clone())
                    },
                    use_kibana: state.use_kibana_host,
                    loop_items: state.loop_text.clone(),
                    extract_path: state.extract_path.clone(),
                    clusters: targets.to_vec(),
                });
                state.show_save_dialog = false;
                state.query_name_input.clear();
            }
            if ui.button("Cancel").clicked() {
                state.show_save_dialog = false;
                state.query_name_input.clear();
            }
        });
    }

    // --- Body editor ---
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Body:");
        if let Some(err) = &state.json_error {
            ui.colored_label(Theme::danger(), format!("⚠️ {}", err));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("✨ Prettify JSON")
                .on_hover_text("Format and auto-indent the JSON body")
                .clicked()
                && !state.body.trim().is_empty()
            {
                match prettify_json_body(&state.body) {
                    Ok(formatted) => {
                        state.body = formatted;
                        state.json_error = None;
                    }
                    Err(e) => state.json_error = Some(e),
                }
            }
        });
    });

    let mut body_h = state.body_height.unwrap_or(200.0);
    let max_body_h = (ui.available_height() - 170.0).max(80.0);
    body_h = body_h.clamp(60.0, max_body_h);
    let res = ui.add_sized(
        [ui.available_width(), body_h],
        egui::TextEdit::multiline(&mut state.body)
            .code_editor()
            .desired_rows(3)
            .layouter(&mut |ui, text, wrap_width| {
                crate::ui::widgets::json_layouter(ui, text, wrap_width)
            }),
    );
    if res.changed() {
        state.json_error = None;
    }

    // Draggable splitter
    ui.add_space(4.0);
    let sep = ui.allocate_response(egui::vec2(ui.available_width(), 8.0), egui::Sense::drag());
    let color = if sep.dragged() {
        Theme::accent()
    } else if sep.hovered() {
        Theme::accent_hover()
    } else {
        Theme::border()
    };
    let y = sep.rect.center().y;
    ui.painter().line_segment(
        [egui::pos2(sep.rect.min.x, y), egui::pos2(sep.rect.max.x, y)],
        egui::Stroke::new(2.0_f32, color),
    );
    if sep.hovered() || sep.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if sep.dragged() {
        body_h += sep.drag_delta().y;
        state.body_height = Some(body_h);
    }
    ui.add_space(4.0);

    // --- Combined response log ---
    ui.horizontal(|ui| {
        ui.label("Response:");
        if state.total_jobs > 0 {
            let pending = state
                .total_jobs
                .saturating_sub(state.ok_count + state.err_count);
            ui.label(
                egui::RichText::new(format!("✅ {}", state.ok_count))
                    .color(Theme::success())
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("❌ {}", state.err_count))
                    .color(if state.err_count > 0 {
                        Theme::danger()
                    } else {
                        Theme::text_muted()
                    })
                    .strong(),
            );
            if state.is_running || pending > 0 {
                ui.label(
                    egui::RichText::new(format!("⏳ {}", pending)).color(Theme::text_secondary()),
                );
            }
        }
        if ui.small_button("📋 Copy all").clicked() {
            ui.ctx().copy_text(state.full_response.clone());
        }
        if ui
            .add_enabled(!state.is_running, egui::Button::new("Clear").small())
            .clicked()
        {
            state.clear_output();
        }
    });

    let response_height = (ui.available_height() - 16.0).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("god_console_response_scroll")
        .max_height(response_height)
        .auto_shrink([false, false])
        .stick_to_bottom(state.is_running)
        .show(ui, |ui| {
            let mut view = state.response.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut view)
                    .code_editor()
                    .desired_width(ui.available_width())
                    .desired_rows(6)
                    .layouter(&mut |ui, text, wrap_width| {
                        crate::ui::widgets::json_layouter(ui, text, wrap_width)
                    }),
            );
        });
}

fn render_confirm_modal(ui: &mut Ui, state: &mut GodConsoleState, actions: &mut GodConsoleActions) {
    let Some(jobs) = state.pending_confirm.as_ref() else {
        return;
    };
    let mut clusters: Vec<&str> = Vec::new();
    for job in jobs {
        if !clusters.contains(&job.cluster.as_str()) {
            clusters.push(&job.cluster);
        }
    }
    let first = jobs
        .first()
        .map(|j| format!("{} {}", j.method, j.path))
        .unwrap_or_default();
    let count = jobs.len();
    let method = jobs.first().map(|j| j.method.clone()).unwrap_or_default();

    let mut decision: Option<bool> = None;
    let modal = egui::Modal::new(egui::Id::new("god_console_confirm")).show(ui.ctx(), |ui| {
        ui.set_max_width(460.0);
        ui.heading(
            egui::RichText::new(format!("⚠️ Send {} {} request(s)?", count, method))
                .color(method_color(&method)),
        );
        ui.add_space(6.0);
        ui.label(if is_read_only_method(&method) {
            format!(
                "This is a large run ({} requests) against {} cluster(s):",
                count,
                clusters.len()
            )
        } else {
            format!(
                "This is a write method and will run against {} cluster(s):",
                clusters.len()
            )
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for c in &clusters {
                    ui.label(egui::RichText::new(format!("• {}", c)).monospace());
                }
            });
        ui.add_space(6.0);
        ui.label(egui::RichText::new("First request:").color(Theme::text_secondary()));
        ui.label(egui::RichText::new(&first).monospace().strong());
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let danger = Theme::danger();
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("Run {} request(s)", count))
                            .color(Theme::contrast_text_color(danger)),
                    )
                    .fill(danger),
                )
                .clicked()
            {
                decision = Some(true);
            }
            if ui.button("Cancel").clicked() {
                decision = Some(false);
            }
        });
    });
    if modal.should_close() && decision.is_none() {
        decision = Some(false);
    }
    match decision {
        Some(true) => {
            if let Some(jobs) = state.pending_confirm.take() {
                state.start_run(jobs, actions);
            }
        }
        Some(false) => state.pending_confirm = None,
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_tuple_style_quoted_list() {
        assert_eq!(parse_loop_items("('a','b','c')"), s(&["a", "b", "c"]));
    }

    #[test]
    fn parses_json_array_and_lines() {
        assert_eq!(parse_loop_items(r#"["x", "y"]"#), s(&["x", "y"]));
        assert_eq!(
            parse_loop_items("logs-a\n  logs-b  \n\nlogs-c\n"),
            s(&["logs-a", "logs-b", "logs-c"])
        );
    }

    #[test]
    fn parses_mixed_and_drops_empties() {
        assert_eq!(
            parse_loop_items("a, b,,\n'c' ,\"d\""),
            s(&["a", "b", "c", "d"])
        );
        assert!(parse_loop_items("   ").is_empty());
        assert!(parse_loop_items("()").is_empty());
    }

    #[test]
    fn keeps_inner_quotes_and_lone_quote() {
        assert_eq!(parse_loop_items("it's"), s(&["it's"]));
        assert_eq!(parse_loop_items("'"), s(&["'"]));
    }

    #[test]
    fn builds_cluster_major_loop_jobs() {
        let jobs = build_jobs(
            &s(&["c1", "c2"]),
            &s(&["a", "b"]),
            &[],
            "POST",
            "/{{loop_item}}/_rollover",
            "",
            false,
            |_| Vec::new(),
        );
        let got: Vec<(String, String)> = jobs
            .iter()
            .map(|j| (j.cluster.clone(), j.path.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("c1".into(), "/a/_rollover".into()),
                ("c1".into(), "/b/_rollover".into()),
                ("c2".into(), "/a/_rollover".into()),
                ("c2".into(), "/b/_rollover".into()),
            ]
        );
        assert!(jobs.iter().all(|j| j.body.is_none()));
        assert_eq!(jobs[1].item.as_deref(), Some("b"));
    }

    #[test]
    fn single_job_per_cluster_without_placeholder() {
        let jobs = build_jobs(
            &s(&["c1", "c2"]),
            &s(&["a", "b", "c"]),
            &[],
            "GET",
            "/_cluster/health",
            "",
            false,
            |_| Vec::new(),
        );
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| j.item.is_none()));
    }

    #[test]
    fn single_job_per_cluster_without_items() {
        let jobs = build_jobs(
            &s(&["c1"]),
            &[],
            &[],
            "GET",
            "/{{loop_item}}",
            "",
            false,
            |_| Vec::new(),
        );
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].path, "/{{loop_item}}");
    }

    #[test]
    fn loop_in_body_only_still_loops() {
        let jobs = build_jobs(
            &s(&["c1"]),
            &s(&["x", "y"]),
            &[],
            "POST",
            "/_aliases",
            r#"{"index":"{{loop_item}}"}"#,
            false,
            |_| Vec::new(),
        );
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[1].body.as_deref(), Some(r#"{"index":"y"}"#));
    }

    #[test]
    fn per_cluster_variables_and_builtins() {
        let vars_for = |c: &str| match c {
            "prod" => vec![("suffix".to_string(), "p".to_string())],
            _ => vec![("suffix".to_string(), "d".to_string())],
        };
        let jobs = build_jobs(
            &s(&["prod", "dev"]),
            &s(&["logs-{{suffix}}"]),
            &[],
            "GET",
            "/{{loop_item}}/_alias?c={{cluster_name}}",
            "",
            false,
            vars_for,
        );
        assert_eq!(jobs[0].path, "/logs-p/_alias?c=prod");
        assert_eq!(jobs[1].path, "/logs-d/_alias?c=dev");
    }

    #[test]
    fn header_format() {
        let job = GodJob {
            cluster: "prod-east".into(),
            item: Some("logs-a".into()),
            result_item: None,
            method: "POST".into(),
            path: "/logs-a/_rollover".into(),
            body: None,
            use_kibana: false,
        };
        assert_eq!(
            format_job_header(&job, true, 142),
            "━━ [prod-east] loop_item=logs-a ━━ POST /logs-a/_rollover → ✅ OK (142 ms)"
        );
        let job = GodJob { item: None, ..job };
        assert!(format_job_header(&job, false, 3).starts_with("━━ [prod-east] ━━ POST"));
    }

    #[test]
    fn results_from_stale_runs_are_ignored() {
        let mut st = GodConsoleState::new();
        st.run_id = 2;
        st.total_jobs = 1;
        let job = GodJob {
            cluster: "c".into(),
            item: None,
            result_item: None,
            method: "GET".into(),
            path: "/".into(),
            body: None,
            use_kibana: false,
        };
        st.push_result(
            1,
            GodJobResult {
                job: job.clone(),
                ok: true,
                text: "old".into(),
                elapsed_ms: 1,
            },
        );
        assert_eq!(st.ok_count, 0);
        st.push_result(
            2,
            GodJobResult {
                job,
                ok: false,
                text: "boom".into(),
                elapsed_ms: 1,
            },
        );
        assert_eq!(st.err_count, 1);
        assert!(st.full_response.contains("boom"));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        assert_eq!(truncate_at_char_boundary("héllo", 2), "h");
        assert_eq!(truncate_at_char_boundary("abc", 10), "abc");
    }

    const SAMPLE: &str = r#"{"toplevel": {"setting": {"ilm": "ilm1", "otherthing": "thingb", "n": 3, "on": true, "gone": null}}}"#;

    fn ex(text: &str, expr: &str) -> ExtractOutcome {
        parse_extraction(expr).unwrap().extract(text)
    }

    #[test]
    fn extracts_scalar_values() {
        let out = ex(SAMPLE, "toplevel.setting.ilm");
        assert_eq!(out.rows, s(&["ilm1"]));
        assert!(out.warnings.is_empty());
        assert_eq!(ex(SAMPLE, "toplevel.setting.n").rows, s(&["3"]));
        assert_eq!(ex(SAMPLE, "$.toplevel.setting.on").rows, s(&["true"]));
    }

    #[test]
    fn missing_null_object_array_and_non_json_warn() {
        let out = ex(SAMPLE, "toplevel.nope");
        assert_eq!(out.rows, s(&["null"]));
        assert!(out.warnings[0].contains("not found"));
        let out = ex(SAMPLE, "toplevel.setting.gone");
        assert_eq!(out.rows, s(&["null"]));
        assert_eq!(out.warnings.len(), 1);
        let out = ex(SAMPLE, "toplevel.setting");
        assert_eq!(out.rows, s(&["object"]));
        assert_eq!(out.warnings.len(), 1);
        let out = ex(r#"{"a":[1,2]}"#, "a");
        assert_eq!(out.rows, s(&["array"]));
        assert_eq!(out.warnings.len(), 1);
        let out = ex("Error: HTTP 500", "a&&b");
        assert_eq!(out.rows, s(&["null && null"]));
        assert!(out.warnings[0].contains("not JSON"));
    }

    /// Trimmed `GET <pattern>/_settings` response: the top-level keys are index names.
    const SETTINGS: &str = r#"{
      ".ds-logs-borderwise.services-wtg-2026.10.23-000011": {"settings": {"index": {
        "lifecycle": {"indexing_complete": "true", "name": "rollover-100gb_or_30d-delete_365d"},
        "provided_name": ".ds-logs-borderwise.services-wtg-2026.10.23-000011"}}},
      ".ds-logs-borderwise.services-wtg-2026.11.03-000014": {"settings": {"index": {
        "lifecycle": {"name": "rollover-100gb_or_30d-delete_365d"},
        "provided_name": ".ds-logs-borderwise.services-wtg-2026.11.03-000014"}}}
    }"#;

    #[test]
    fn top_level_wildcard_with_key_capture() {
        let out = ex(SETTINGS, "*&&*.settings.index.lifecycle.name");
        assert_eq!(
            out.rows,
            s(&[
                ".ds-logs-borderwise.services-wtg-2026.10.23-000011 && rollover-100gb_or_30d-delete_365d",
                ".ds-logs-borderwise.services-wtg-2026.11.03-000014 && rollover-100gb_or_30d-delete_365d",
            ])
        );
        assert!(out.warnings.is_empty());
        let out = ex(
            SETTINGS,
            "*.settings.index.provided_name && *.settings.index.lifecycle.name",
        );
        assert_eq!(out.rows.len(), 2);
        assert!(out.rows[0].starts_with(".ds-logs-borderwise.services-wtg-2026.10.23-000011 && "));
    }

    #[test]
    fn missing_field_in_one_row_warns_and_keeps_row() {
        let out = ex(SETTINGS, "*&&*.settings.index.lifecycle.indexing_complete");
        assert_eq!(out.rows.len(), 2);
        assert!(out.rows[0].ends_with(" && true"));
        assert!(out.rows[1].ends_with(" && null"));
        assert_eq!(out.warnings.len(), 1);
        assert!(out.warnings[0].contains("2026.11.03-000014"));
        assert!(row_has_marker(&out.rows[1]));
        assert!(!row_has_marker(&out.rows[0]));
    }

    #[test]
    fn nested_and_array_wildcards_with_fixed_columns() {
        let json = r#"{"cluster_name":"prod","indices":{"a":{"policy":"p1"},"b":{"policy":"p2"}},
                       "hits":{"hits":[{"_id":"x"},{"_id":"y"}]}}"#;
        assert_eq!(
            ex(json, "cluster_name&&indices.*&&indices.*.policy").rows,
            s(&["prod && a && p1", "prod && b && p2"])
        );
        assert_eq!(
            ex(json, "hits.hits.*&&hits.hits[*]._id").rows,
            s(&["0 && x", "1 && y"])
        );
        let out = ex(r#"{"indices":{}}"#, "indices.*&&indices.*.policy");
        assert_eq!(out.rows, s(&["null && null"]));
        assert!(out.warnings[0].contains("matched nothing"));
        let out = ex(r#"{}"#, "indices.*");
        assert!(out.warnings[0].contains("not found"));
        let out = ex(r#"{"indices":5}"#, "indices.*");
        assert!(out.warnings[0].contains("not an object or array"));
    }

    #[test]
    fn extraction_rules_are_enforced() {
        assert!(
            parse_extraction("a.*.b.*")
                .unwrap_err()
                .contains("more than one")
        );
        assert!(
            parse_extraction("ind*.x")
                .unwrap_err()
                .contains("whole path segment")
        );
        assert!(
            parse_extraction("a.*&&b.*")
                .unwrap_err()
                .contains("same path")
        );
        assert!(parse_extraction("a&&").unwrap_err().contains("empty path"));
        assert!(parse_extraction("a.*&&a.*.x&&c").is_ok());
        assert_eq!(parse_extraction("a && b && c").unwrap().columns(), 3);
    }

    #[test]
    fn values_containing_separator_or_newlines_stay_one_field() {
        let out = ex(r#"{"v":"a && b\nc"}"#, "v");
        assert_eq!(out.rows, s(&["a & & b c"]));
        assert_eq!(split_row(&out.rows[0]).len(), 1);
    }

    #[test]
    fn numbered_results_fields_substitute_in_one_pass() {
        let row = "my-index && {{results_item.1}} && hot";
        assert_eq!(
            substitute_results(
                "/{{results_item}}/x/{{results_item.2}}/{{results_item.3}}",
                row
            ),
            "/my-index/x/{{results_item.1}}/hot"
        );
        assert_eq!(
            substitute_results(
                "{{results_item.4}} {{results_item.0}} {{results_item.x}}",
                row
            ),
            "{{results_item.4}} {{results_item.0}} {{results_item.x}}"
        );
        assert_eq!(
            max_results_field("/{{results_item.12}}/{{results_item}}"),
            12
        );
        assert_eq!(max_results_field("/{{results_items}}"), 0);
        assert!(uses_results("/_ilm/policy/{{results_item.2}}", ""));
        assert!(!uses_results("/x", "{}"));
    }

    #[test]
    fn path_indexes_arrays_and_dotted_keys() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"hits":{"hits":[{"_id":"x"},{"_id":"y"}]},
                "indices":{".ds-logs-2026.11.03-000001":{"policy":"hot-warm"}}}"#,
        )
        .unwrap();
        assert_eq!(lookup_json_path(&json, "hits.hits.1._id").unwrap(), "y");
        assert_eq!(lookup_json_path(&json, "hits.hits[0]._id").unwrap(), "x");
        // A key starting with "." is written after the parent's separator: `indices..ds-…`.
        assert_eq!(
            lookup_json_path(&json, "indices..ds-logs-2026.11.03-000001.policy").unwrap(),
            "hot-warm"
        );
        assert_eq!(
            lookup_json_path(&json, "indices.ds-logs-2026.11.03-000001.policy"),
            None
        );
        let json2: serde_json::Value =
            serde_json::from_str(r#"{".ds-logs-2026.11.03":{"policy":"p"}}"#).unwrap();
        assert_eq!(
            lookup_json_path(&json2, ".ds-logs-2026.11.03.policy").unwrap(),
            "p"
        );
        assert_eq!(lookup_json_path(&json, "hits.hits.9._id"), None);
    }

    #[test]
    fn extracted_items_split_on_lines_only() {
        assert_eq!(parse_extracted_items("a, b\n\n  c  \n"), s(&["a, b", "c"]));
    }

    #[test]
    fn results_multiply_jobs_after_loop_items() {
        let jobs = build_jobs(
            &s(&["c1"]),
            &s(&["a", "b"]),
            &s(&["p1", "p2"]),
            "GET",
            "/{{loop_item}}/{{results_item}}",
            "",
            false,
            |_| Vec::new(),
        );
        let paths: Vec<&str> = jobs.iter().map(|j| j.path.as_str()).collect();
        assert_eq!(paths, vec!["/a/p1", "/a/p2", "/b/p1", "/b/p2"]);
        assert_eq!(jobs[1].result_item.as_deref(), Some("p2"));
        assert!(format_job_header(&jobs[1], true, 1).contains(" loop_item=a results_item=p2 "));
    }

    #[test]
    fn results_are_not_reinterpolated_as_variables() {
        let jobs = build_jobs(
            &s(&["c1"]),
            &[],
            &s(&["{{secret}}"]),
            "GET",
            "/_ilm/policy/{{results_item}}",
            "",
            false,
            |_| vec![("secret".to_string(), "leaked".to_string())],
        );
        assert_eq!(jobs[0].path, "/_ilm/policy/{{secret}}");
    }

    fn ok_result(cluster: &str, text: &str) -> GodJobResult {
        GodJobResult {
            job: GodJob {
                cluster: cluster.into(),
                item: None,
                result_item: None,
                method: "GET".into(),
                path: "/".into(),
                body: None,
                use_kibana: false,
            },
            ok: true,
            text: text.into(),
            elapsed_ms: 1,
        }
    }

    #[test]
    fn run_fills_extracted_list_with_warnings() {
        let mut st = GodConsoleState::new();
        st.extract_path = "toplevel.setting.ilm".into();
        st.extracted_text = "stale\n".into();
        let mut actions = GodConsoleActions::default();
        st.start_run(Vec::new(), &mut actions);
        let id = st.run_id;
        st.push_result(id, ok_result("c1", SAMPLE));
        st.push_result(id, ok_result("c2", r#"{"toplevel":{}}"#));
        let mut failed = ok_result("c3", "Error: boom");
        failed.ok = false;
        st.push_result(id, failed);
        assert_eq!(st.extracted_text, "ilm1\nnull\nnull\n");
        assert_eq!(st.extract_warnings.len(), 2);
        assert!(st.full_response.contains("🔎 Extracted: ilm1"));
        assert!(
            st.full_response
                .contains("⚠️ Extraction: request failed → null")
        );
    }

    #[test]
    fn consuming_run_does_not_overwrite_extracted_list() {
        let mut st = GodConsoleState::new();
        st.extract_path = "toplevel.setting.ilm".into();
        st.extracted_text = "ilm1\n".into();
        st.path = "/_ilm/policy/{{results_item}}".into();
        let mut actions = GodConsoleActions::default();
        st.start_run(Vec::new(), &mut actions);
        let id = st.run_id;
        st.push_result(id, ok_result("c1", SAMPLE));
        assert_eq!(st.extracted_text, "ilm1\n");
        assert!(!st.full_response.contains("Extract"));
    }
}
