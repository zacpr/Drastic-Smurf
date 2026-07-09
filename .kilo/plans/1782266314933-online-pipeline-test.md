# Plan: Pipeline Sandpit rename + "Online Pipeline Test" tab

## Goal
Rename the existing local Pipeline Simulator screen to **Pipeline Sandpit** and add a new **Online Pipeline Test** sub-tab that wraps the Elasticsearch **Simulate data ingestion** API (`POST /_ingest/{index}/_simulate`) in a convenient, user-friendly 3-column UI. The new tab lets you pick a cluster + index/data stream, choose a pipeline (default or loaded from the cluster), supply test documents, run the simulation, and persist/reload named test settings.

## Resolved decisions
1. **File picker** → add the `rfd` crate for a native OS file-open dialog ("Load Documents from File").
2. **Pipeline modes** (verified vs ES docs):
   - **Default** → `POST /_ingest/{target}/_simulate` body `{ "docs": [...] }` (uses the cluster's stored pipelines).
   - **Loaded** → fetch `GET /_ingest/pipeline/{id}`, show its definition in the editable Pipeline box, then send it back via the **`pipeline_substitutions`** body field: `{ "docs": [...], "pipeline_substitutions": { "<id>": <edited_def> } }`. Substitutions are request-scoped and override stored pipeline definitions only for this simulate call (per official API semantics).
3. **Test settings persistence** → named presets, stored **globally** in `AppConfig.pipeline_test_presets`.
4. **Container naming** → top tab renamed **"Pipelines"**; sub-tabs **"🧪 Pipeline Sandpit"** (existing local engine) and **"🌐 Online Pipeline Test"** (new).
5. **Index/datastream picker** → a custom **filterable dropdown** widget (type-to-narrow), since egui `ComboBox` can't type-filter.
6. **Documents format** → a single editable JSON array `[ { "_source": {...} }, ... ]` matching the simulate `docs` field.
7. **Target type** → index/data stream/alias all go to the same `/_ingest/{target}/_simulate` endpoint; the radio just chooses which list populates the picker (indices vs data streams).

## Affected files
- `Cargo.toml` — add `rfd`.
- `src/core/es_client.rs` — new client methods.
- `src/core/config.rs` — `PipelineTestPreset` struct + `AppConfig` field.
- `src/app.rs` — tab rename, sub-tab dispatch, new state field, new `RefreshMsg` variants + handlers, async task spawning, preset load/save wiring.
- `src/modules/mod.rs` — register `pipeline_online`.
- `src/modules/pipeline.rs` — heading rename ("Pipeline Simulator" → "Pipeline Sandpit"), sub-tab switcher at the top of `render_pipeline_module` path (see wiring note).
- `src/modules/pipeline_online.rs` — **new** module: the Online Pipeline Test UI.
- `src/ui/widgets.rs` — new `filterable_select` helper widget.
- `AGENTS.md` — update Pipeline module description (optional, after).

---

## Implementation steps

### Step 1 — Add `rfd` dependency
In `Cargo.toml` `[dependencies]`, add:
```toml
rfd = "0.15"
```
Verify it builds across the existing `cfg(target_os)` sections (rfd is cross-platform; no per-target config needed). Run `cargo check --all-targets`.

### Step 2 — New ES client methods (`src/core/es_client.rs`)
Add to `impl EsClient` (use the existing `execute`/`request` helpers):
- `pub async fn get_ingest_pipelines(&self) -> Result<serde_json::Value, EsError>`
  → `GET /_ingest/pipeline` (returns an object whose keys are pipeline ids).
- `pub async fn get_ingest_pipeline(&self, id: &str) -> Result<serde_json::Value, EsError>`
  → `GET /_ingest/pipeline/{id}` (returns `{ "<id>": { <definition> } }`).
- `pub async fn simulate_ingest(&self, target: &str, body: serde_json::Value) -> Result<serde_json::Value, EsError>`
  → `POST /_ingest/{target}/_simulate` with JSON body (the `execute` POST helper already sets `Content-Type: application/json`).
- Doc fetch reuses the generic `execute(GET, "/{index}/_doc/{id}", None)` — no new method strictly required; the app layer will call `client.execute(...)` and extract `_source`.

### Step 3 — Config types (`src/core/config.rs`)
Add:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PipelineTargetKind { Index, DataStream }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PipelineMode { Default, Loaded } // Loaded carries the id inside the preset text/field

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTestPreset {
    pub name: String,
    pub cluster: String,
    pub target_kind: PipelineTargetKind,
    pub target_name: String,
    pub pipeline_mode: PipelineMode,
    pub pipeline_id: Option<String>,   // when Loaded
    pub pipeline_text: String,         // pipeline def shown in the box (Loaded mode)
    pub docs_text: String,             // the JSON-array documents box contents
}
```
Add to `AppConfig`:
```rust
#[serde(default)]
pub pipeline_test_presets: Vec<PipelineTestPreset>,
```
Add accessors on `ClusterManager` mirroring pinned-monitor handling: `pipeline_test_presets() -> Vec<PipelineTestPreset>` and `save_pipeline_test_presets(v) -> Result<()>` (store in an `Arc<Mutex<Vec<...>>>`, set in `load()`/`save()` alongside the other fields, and `mark_dirty()`).

### Step 4 — Filterable dropdown widget (`src/ui/widgets.rs`)
Add a reusable helper `filterable_select`:
- Signature roughly: `fn filterable_select(ui, id: impl Hash, selected: &mut String, filter: &mut String, options: &[String]) -> bool` returning `true` when a selection was made.
- Implementation: a `Button` showing the current selection (or placeholder); when clicked, open an `egui::Popup` (or `Area`) containing a `TextEdit` (bound to `filter`) and a scrollable list of `options.iter().filter(contains filter).selectable_value(...)`. Clicking an option sets `selected`, clears filter, closes the popup.
- Keep it self-contained and theme-consistent (`Theme::*` colors).

### Step 5 — Online module (`src/modules/pipeline_online.rs`)
State:
```rust
#[derive(Debug, Clone)]
pub struct OnlinePipelineState {
    pub selected_cluster: String,
    pub target_kind: PipelineTargetKind,    // radio: Index | DataStream
    pub target_name: String,
    pub target_filter: String,              // for filterable_select
    pub indices: Vec<String>,               // cached index names for selected cluster
    pub datastreams: Vec<String>,           // cached data stream names
    pub pipeline_mode: PipelineMode,
    pub pipeline_id: String,
    pub pipeline_filter: String,
    pub pipeline_ids: Vec<String>,          // cached pipeline ids for selected cluster
    pub pipeline_text: String,              // editable JSON definition box
    pub docs_text: String,                  // editable JSON array of {"_source":{}}
    pub docs_error: Option<String>,
    pub result_text: String,                // prettified simulate response
    pub is_loading: bool,
    pub indices_loading: bool,
    pub pipelines_loading: bool,
    // doc-fetch modal
    pub show_doc_modal: bool,
    pub doc_modal_index: String,
    pub doc_modal_id: String,
    // preset save modal
    pub show_save_modal: bool,
    pub preset_name_input: String,
}
```
Render function `render_online_pipeline_module(ui, state, clusters, on_action, ...)` where `on_action` is an enum of intents the app layer turns into async tasks (mirrors the existing `*_send`/`on_refresh` Option-flag pattern):
- `FetchTargets(cluster)` — fetch indices + data streams for cluster.
- `FetchPipelines(cluster)` — fetch pipeline id list for cluster.
- `LoadPipelineDef(cluster, id)` — fetch one pipeline def into `pipeline_text`.
- `FetchDoc(cluster, index, id)` — get a doc's `_source` to append.
- `PickFile` — open rfd dialog, read file, append to docs.
- `Simulate(cluster, target, mode, pipeline_id, pipeline_text, docs_text)` — run the simulate call.
- `SavePreset(name)` / `LoadPreset(name)` / `DeletePreset(name)` — preset ops.

Layout (top bar + 3 columns, mirroring `render_pipeline_module` proportions ~0.36/0.32/0.32):
- **Top bar**: cluster ComboBox (reuse indices pattern, app.rs:132); Indices / DataStreams radio (`ui.radio_value`); filterable target selector (`filterable_select`); spacer; "💾 Save Test Settings", "📂 Load Test Settings".
- **Column 1 "Pipeline"**: `ui.radio_value` for `Default | Loaded`; if `Loaded`, a pipeline ComboBox/populated filterable select over `pipeline_ids` + a "Load Definition" button (sets `LoadPipelineDef`); a `TextEdit::multiline().code_editor()` bound to `pipeline_text` with the JSON layouter (reuse `crate::ui::widgets::json_layouter`). Show live JSON parse error if any.
- **Column 2 "Test Documents"**: buttons row — "📁 Load from File" (PickFile), "🔎 Load from Elastic" (open `show_doc_modal`), "🗑 Clear Documents" (clears `docs_text`); `TextEdit::multiline().code_editor()` bound to `docs_text`; `docs_error` shown in red; "🔄 Run Simulation" button (fills width, success-colored, like pipeline.rs:181).
- **Column 3 "Pipeline Result"**: `ScrollArea` + read-only `TextEdit` bound to `result_text` (json layouter), like `render_pipeline_output`.
- **Doc modal**: `egui::Window` anchored center (mirror `render_add_cluster_dialog`) with `_id` + `index` `TextEdit`s and Load/Cancel buttons → sets `FetchDoc`.
- **Save preset modal**: `egui::Window` with a name `TextEdit` + Save/Cancel; if name exists, overwrite (confirm via toast).

Doc-append logic: parse `docs_text` as a JSON array; push `{"_source": <fetched_source>}`; re-serialize pretty. On parse failure of existing box, fall back to error (don't clobber). File-load: read file text; if it parses as a JSON array, append each element; if it parses as a single object, wrap; else append raw as one doc. Always **append**, never replace.

### Step 6 — App wiring (`src/app.rs`)
- Rename tab label in `render_tabs` (app.rs:1382): `("Pipeline Simulator", Tab::PipelineSimulator)` → `("Pipelines", Tab::PipelineSimulator)`. (Keep the enum variant name; only the label changes — or rename variant to `Pipelines`. Recommended: rename variant `Tab::Pipelines` and update all references.)
- Add `pub pipeline_online_state: OnlinePipelineState`, `pub pipeline_sub_tab: PipelineSubTab` (or store sub-tab inside a small wrapper) to `DrasticSmurfApp`; init in `with_log_entries`.
- In `Tab::Pipelines` dispatch: render a sub-tab header (`🧪 Pipeline Sandpit` / `🌐 Online Pipeline Test`) then call either `render_pipeline_module` (with renamed heading) or `render_online_pipeline_module`.
- Pipe `clusters: Vec<String>` (filtered cluster names) into the online renderer.
- Add `RefreshMsg` variants:
  - `OnlineIndicesResult(String cluster, Vec<String> indices, Vec<String> datastreams)`
  - `OnlinePipelinesResult(String cluster, Vec<String> ids)`
  - `OnlinePipelineDefResult(String cluster, String id, Result<String, String>)`  (prettified def text or error)
  - `OnlineDocResult(Result<serde_json::Value, String>)`
  - `OnlineSimulateResult(Result<String, String>)`  (prettified response or error)
- Handlers in `process_refresh_results`: update `pipeline_online_state`, set loading flags false, surface errors via `self.toasts`.
- Action spawning (same pattern as console discover_send / indices_refresh): on each returned intent, clone `cluster_manager` + `refresh_tx` + `ctx`, `tokio::spawn`, call client method, `tx.send(...)`, `ctx.request_repaint()`. For `PickFile`, use `rfd::AsyncFileDialog::new().add_filter("JSON", &["json","ndjson","txt"]).pick_file().await`, then read file and send `OnlineDocResult`-style content (or a dedicated `OnlineFileResult(String)` variant). For `Simulate`, build the body:
  - Parse `docs_text` as `serde_json::Value::Array`; on error send `OnlineSimulateResult(Err(...))`.
  - Default → `json!({ "docs": <array> })`.
  - Loaded → parse `pipeline_text` as a Value; `json!({ "docs": <array>, "pipeline_substitutions": { <id>: <def> } })`.
  - `client.simulate_ingest(target, body).await`; on success prettify `serde_json::to_string_pretty`; on error map `EsError` to string.
- Preset ops: Save → push/update into `cluster_manager.pipeline_test_presets()` (via `save_pipeline_test_presets`), `mark_dirty`. Load → find preset by name, populate `pipeline_online_state` fields and trigger `FetchTargets`/`FetchPipelines` for that cluster. Delete → filter out and save.
- Truncate `result_text` > 100k like console (app.rs:671).

### Step 7 — Sandpit heading rename (`src/modules/pipeline.rs`)
Change `ui.heading("Pipeline Simulator")` (pipeline.rs:94) → `ui.heading("Pipeline Sandpit")`.

### Step 8 — Register module (`src/modules/mod.rs`)
Add `pub mod pipeline_online;`.

### Step 9 — Validation
- `cargo check --all-targets` (zero warnings target per AGENTS.md).
- `cargo clippy --all-targets`.
- `cargo test` (existing tests must pass).
- Add one unit test in `pipeline_online.rs` for the simulate body builder (Default vs Loaded → expected JSON, incl. `pipeline_substitutions` key presence) — pure function, no network.

## Risks / edge cases
- **rfd build**: pulls platform backends (GTK on Linux). Confirm a clean `cargo build` on Linux/Windows/macOS; if GTK is unavailable in CI it may need the `rfd` `xdg-portal` feature instead — verify after adding.
- **Target type**: simulate accepts index/alias/data stream name in `{index}` path — no special handling, but if the data stream has no default pipeline, results may be trivial; surface the raw response regardless.
- **Large responses**: truncate `result_text` > 100k chars (consistent with console).
- **Pipeline def format**: `GET /_ingest/pipeline/{id}` returns `{ "<id>": { "description":..., "processors":[...] } }`; when loading, extract the inner definition (`[id]`) into the box and re-wrap by id when sending as substitution.
- **Docs box invalid JSON**: never clobber user edits; show parse error, disable Run until valid.
- **No cluster selected / empty target list**: disable Run + show hint.

## Out of scope
- Console-style JSON syntax highlighting / folding in the result box (existing prettify only).
- Editing/PUT-ting the loaded pipeline back to the cluster (read-only simulate).
- Component-template / index-template substitutions (only `pipeline_substitutions` per the decision).

## Validation checklist
- [ ] Tab reads "Pipelines"; Sandpit heading reads "Pipeline Sandpit".
- [ ] Online tab: selecting cluster loads index + data stream lists; filterable picker narrows by typing.
- [ ] Default mode simulate posts `/_ingest/{target}/_simulate` with `{docs}`; result pretty-prints.
- [ ] Loaded mode: pipeline dropdown populated; "Load Definition" fills the box; simulate includes `pipeline_substitutions`.
- [ ] Load from File appends (rfd dialog); Load from Elastic appends fetched `_source`; Clear empties the box.
- [ ] Save/Load/Delete named presets persist across restarts (config.json).
- [ ] `cargo check --all-targets`, `cargo clippy --all-targets`, `cargo test` all pass.
