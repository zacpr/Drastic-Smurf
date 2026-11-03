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
        let mut body = result.text;
        if body.len() > JOB_DISPLAY_LIMIT {
            let cut = truncate_at_char_boundary(&body, JOB_DISPLAY_LIMIT).len();
            body.truncate(cut);
            body.push_str("\n... [Response truncated. Use 'Copy all' for the full payload] ...");
        }
        let block = format!("{}\n{}\n\n", header, body);
        self.append_text(&block);
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

/// Whether the request template references the loop placeholder.
pub fn uses_loop(path: &str, body: &str) -> bool {
    path.contains(LOOP_PLACEHOLDER) || body.contains(LOOP_PLACEHOLDER)
}

/// Interpolate a template for one cluster and optional loop item.
///
/// Order: built-ins (`{{loop_item}}`, `{{cluster_name}}`) first, then the cluster's own
/// Console variables. This lets a loop value itself contain a `{{var}}` reference.
pub fn interpolate_for(
    template: &str,
    cluster: &str,
    item: Option<&str>,
    cluster_vars: &[(String, String)],
) -> String {
    let mut out = template.replace(CLUSTER_PLACEHOLDER, cluster);
    if let Some(item) = item {
        out = out.replace(LOOP_PLACEHOLDER, item);
    }
    interpolate_variables(&out, cluster_vars)
}

/// Expand a request template into concrete jobs: for each cluster (in order), each loop item.
///
/// When there are no loop items, or the template doesn't use `{{loop_item}}`, each cluster
/// gets exactly one job.
pub fn build_jobs(
    clusters: &[String],
    items: &[String],
    method: &str,
    path: &str,
    body: &str,
    use_kibana: bool,
    vars_for: impl Fn(&str) -> Vec<(String, String)>,
) -> Vec<GodJob> {
    let looping = !items.is_empty() && uses_loop(path, body);
    let mut jobs = Vec::new();
    for cluster in clusters {
        let vars = vars_for(cluster);
        let mut push = |item: Option<&str>| {
            let interp_body = interpolate_for(body, cluster, item, &vars);
            jobs.push(GodJob {
                cluster: cluster.clone(),
                item: item.map(str::to_string),
                method: method.to_string(),
                path: interpolate_for(path, cluster, item, &vars),
                body: if interp_body.trim().is_empty() {
                    None
                } else {
                    Some(interp_body)
                },
                use_kibana,
            });
        };
        if looping {
            for item in items {
                push(Some(item));
            }
        } else {
            push(None);
        }
    }
    jobs
}

/// The one-line header written above each job's output in the combined log.
pub fn format_job_header(job: &GodJob, ok: bool, elapsed_ms: u128) -> String {
    let item = match &job.item {
        Some(i) => format!(" loop_item={}", i),
        None => String::new(),
    };
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
    let looping = !items.is_empty() && uses_loop(&state.path, &state.body);
    let per_cluster = if looping { items.len() } else { 1 };
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
                        render_workspace(ui, state, clusters, &items, actions);
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
                            ui, state, &targets, &items, looping, job_count, vars_for, actions,
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
            GodTab::Targets => render_targets_tab(ui, state, clusters, items),
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
            "Clusters: {}\nLoop values: {}\nBody: {}",
            entry.clusters.join(", "),
            entry.loop_text.replace('\n', ", "),
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
                "{} {}\nClusters: {}\nLoop values: {}",
                query.method,
                query.path,
                query.clusters.join(", "),
                query.loop_items.replace('\n', ", ")
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
    looping: bool,
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
            } else if looping {
                egui::RichText::new(format!(
                    "🎯 {} cluster(s) × {} item(s) = {} request(s)",
                    targets.len(),
                    items.len(),
                    job_count
                ))
                .color(Theme::accent())
            } else {
                egui::RichText::new(format!(
                    "🎯 {} cluster(s) = {} request(s)",
                    targets.len(),
                    job_count
                ))
                .color(Theme::accent())
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
                    if !is_read_only_method(&state.method) && jobs.len() > 1 {
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
                        let item = job
                            .item
                            .as_ref()
                            .map(|i| format!(" loop_item={}", i))
                            .unwrap_or_default();
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
        let vars = vars_for(first);
        let evaluated = interpolate_for(&state.path, first, item, &vars);
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
        ui.label(format!(
            "This is a write method and will run against {} cluster(s):",
            clusters.len()
        ));
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
        let jobs = build_jobs(&s(&["c1"]), &[], "GET", "/{{loop_item}}", "", false, |_| {
            Vec::new()
        });
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].path, "/{{loop_item}}");
    }

    #[test]
    fn loop_in_body_only_still_loops() {
        let jobs = build_jobs(
            &s(&["c1"]),
            &s(&["x", "y"]),
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
}
