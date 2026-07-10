v# AGENTS.md — DRASTIC SMURF

## Project Overview

**Status:** Active development — core architecture complete, all four planned modules have working skeletons.  
**Goal:** An extensible desktop GUI for monitoring and interacting with multiple Elasticsearch clusters. Supports multiple clusters with separate credentials, a tabbed modular interface, and secure credential storage.

### Technology Stack

- **Rust** (edition 2024)
- **egui** + **eframe** (immediate-mode GUI, wgpu backend)
- **tokio** (async runtime)
- **reqwest** (async HTTP client)
- **serde/serde_json** (JSON parsing)
- **keyring** (OS-native secret storage)

### Modules

| Module | Status | Description |
|--------|--------|-------------|
| **Snapshot Monitoring** | ✅ Functional | Cards with progress bars, speed/ETA, sparklines, SLM policy info. Modeled after `es-snap-mon`. Responsive 1→2 column layout. |
| **Cluster Status** | ✅ Functional | Health dashboard with nodes, shards, indices, docs, store size, JVM heap. Responsive 1→2 column layout. |
| **Dashboard** | ✅ Functional | Multi-tab cluster dashboard with Overview grid and detailed Single Cluster observability (nodes list, shard allocation details, JVM heap, index statistics). |
| **Task Monitoring** | ✅ Functional | Filterable task grid (cluster, action, description, running time, cancellable) with dynamic category dropdown filtering. |
| **Clusters** | ✅ Functional | Centralized cluster management: list, add/edit, test connection, import/export. |
| **Elastic Console** | ✅ Enhanced | Category-based Elasticsearch & Kibana presets (40+ items), official documentation links, automatic connection target host toggles (ES vs Kibana), custom variables with interpolation, command history cycling, saved queries, JSON body prettification, and live validation error reporting. |
| **Discover** | ✅ Functional | A cutdown mimic of Kibana's Discover. Index pattern interrogating, Lucene/KQL search queries, dynamic available fields selection, and collapsible document JSON drawers. |
| **Datastreams & Indices** | ✅ Functional | Sub-tabbed browser for Indices and Data Streams, displaying names, status, health states, doc counts, and store sizes. Supports multi-selection checkbox tracking and selection-only filtering. |
| **Observability Monitors** | ✅ Functional | Kibana Synthetics / Uptime monitors explorer with space-selection support, multi-region status, sparkline history latency, and a customizable Pinned Dashboard. |
| **World Clocks** | ✅ Functional | Collapsible Sidebar World Timezones displaying ISO 8601 local, UTC, Sydney (APAC), Germany (EMEA), Chicago (AMER), and custom offsets. Supports labels modification, clipboard copy buttons, toggles, deletion, and full Settings Tab persistence. |
| **AI Assistant** | ✅ Functional | Side-dock chat panel for troubleshooting Elasticsearch and API usage. Streams from any OpenAI-compatible `/chat/completions` endpoint (OpenAI, Azure OpenAI, GitHub Copilot, Ollama, LM Studio, vLLM, OpenRouter, etc.). API keys stored in OS keyring, conversation history persisted in `config.json`, concise auto-injected cluster context (health/version/nodes/JVM/errors). SSE streaming with token-by-token UI updates and cancel support. |
| **Painless Playground** | ✅ Functional | Elasticsearch Painless script editor featuring preset templates (Math, Score, Filter, String manipulation), live JSON parameters validation, and full mock context indexing setup. |


---

## Project Layout

```
src/
├── app.rs              # Main app state, tab switching, refresh orchestration
├── main.rs             # Entry point, eframe setup
├── core/
│   ├── auth.rs         # Keyring-based password/API token storage
│   ├── cluster_manager.rs  # Cluster CRUD, client caching, tunnel lifecycle
│   ├── config.rs       # ClusterConfig, AppConfig, save/load
│   ├── es_client.rs    # Async ES HTTP client + all response models
│   ├── llm.rs          # OpenAI-compatible LLM client + SSE streaming
│   ├── mod.rs
│   └── ssh_tunnel.rs   # SSH tunnel spawning via `ssh -L`
├── modules/
│   ├── clusters.rs     # Clusters management tab
│   ├── console.rs      # Elastic Console tab
│   ├── dashboard.rs    # Cluster Dashboard tab (Overview & Detail views)
│   ├── discover.rs     # Discover tab
│   ├── indices.rs      # Datastreams & Indices explorer tab
│   ├── llm_assistant.rs # AI Assistant side-dock panel
│   ├── observability.rs # Kibana Synthetics Monitors tab
│   ├── painless.rs     # Painless Script Playground tab
│   ├── snapshot.rs     # Snapshot Monitoring tab
│   ├── status.rs       # Cluster Status tab
│   ├── tasks.rs        # Task Monitoring tab
│   └── mod.rs
└── ui/
    ├── theme.rs        # Color palette, health/snapshot state colors
    ├── widgets.rs      # GradientProgressBar, MiniSparkline, ConnectionDot, StatePill
    └── mod.rs
```

---

## Build and Test Commands

```bash
# Check
$ cargo check --all-targets

# Build debug
$ cargo build

# Build release
$ cargo build --release

# Build Windows release (cross-compile)
$ cargo build --release --target x86_64-pc-windows-gnu

# Run
$ cargo run
```

### Packaging (requires release binary)

```bash
# Debian/Ubuntu
$ cargo deb

# RHEL/Fedora
$ cargo generate-rpm
```

---

## Code Style

- **rustfmt** for formatting
- **clippy** for linting (`cargo clippy --all-targets`)
- Aim for zero warnings on `cargo check --all-targets`
- Prefer `#[allow(dead_code)]` on API/model code that is intentionally reserved for future use, rather than deleting it.

---

## Testing

- **Unit tests** — implemented for console variable interpolation; planned for JSON parsing, stat translation, and utility functions
- **Integration tests** — planned against a local Elasticsearch instance or mock HTTP server
- **UI tests** — limited; egui does not have a built-in UI testing framework

*(Unit tests are now partially implemented — console interpolation has working tests.)*

---

## Security Considerations

- **Do not commit credentials.** Passwords and API keys are stored in the OS keyring.
- Use `directories` crate for config storage (`~/.config/drastic-smurf/` on Linux).
- **Export JSON never contains passwords.** Exported cluster configs omit credentials; imported clusters require password re-entry.
- Per-cluster cached module data (status history, tasks cache, snapshot cache, saved queries) is stored in `config.json` alongside cluster configs.
- Application state flags, including theme choices, VFX toggles, and onboarding tour status (`wizard_completed`), are persisted in `config.json`.
- TLS verification is on by default; per-cluster override available.
- Custom CA certificate support is partially implemented (`CaCert::Custom` works; `CaCert::Bundled` is a TODO).
- API token auth methods are stubbed in `auth.rs` but not yet wired into `EsClient`.

---

## CI / Release

### GitHub Actions Workflows

| Workflow | Trigger | What it does |
|----------|---------|--------------|
| `ci.yml` | Push/PR to `main`/`master` | `cargo check`, `cargo test`, `cargo fmt --check`, `cargo clippy` |
| `release.yml` | Tag push (`v*.*.*`) | cargo-dist builds archives + MSI installers + DEB/RPM packages + shell/PowerShell installers, and creates the GitHub Release |

### Releasing

1. Bump version in `Cargo.toml` (and `Cargo.lock` via `cargo check`)
2. Commit and tag: `git tag v0.x.y`
3. Push tag: `git push origin v0.x.y`
4. cargo-dist creates the release automatically with all built packages.

---

## Next Steps / Known Gaps

1. **Tests** — Add unit tests for snapshot stat calculations and config roundtrips. (Completed: `human_duration`, `human_nanos`, `human_bytes`, and `human_docs` formatting helpers are now fully tested!)
2. **Status module depth** — (Completed: Added a brand new tabbed Dashboard module containing high-level cluster grids and detailed single-cluster views with node list, shards distribution table, index stats, and JVM heap charts!)
3. **Task type filtering** — (Completed: Task-type dropdown filtering is fully implemented, allowing users to select and filter by action namespaces like indices, cluster, or transport!)
4. **Console enhancements** — No JSON syntax highlighting or response folding. (Completed: Prettify/Format JSON button with live parser validation error reporting is now fully functional!)
5. **Passphrase-encrypted export** — Export is currently plaintext JSON without passwords. Encrypted export could be added later.
6. **AGENTS.md upkeep** — Update this file whenever modules, build steps, or security boundaries change.

---

## Known Issues & Workarounds

### Linux IME text input bug (all `TextEdit` widgets)

**Symptom:** On Linux (especially Wayland/Flatpak), every text input field in the app only accepts a single character. After typing one character, the field appears "locked" — further keystrokes are silently swallowed. Backspace and paste still work. This affects ALL text boxes (cluster filter, indices filter, console, cluster config dialog, etc.).

**Root cause:** egui's `TextEdit` widget sets `PlatformOutput::ime = Some(...)` whenever it gains focus. `egui-winit` sees this and calls `window.set_ime_allowed(true)`, activating the OS input method editor (IME). On Linux, the IME (ibus/fcitx/Wayland text-input protocol) intercepts all keyboard input and its composition state machine gets stuck after the first character — subsequent keystrokes are treated as incomplete composition events and never committed to the text buffer.

**Fix:** At the very end of `DrasticSmurfApp::update()` (in `src/app.rs`), after all UI rendering, clear the IME output so egui-winit keeps IME disabled:

```rust
#[cfg(not(target_arch = "wasm32"))]
{
    ctx.output_mut(|o| o.ime = None);
}
```

This forces text input to go through direct keyboard/character events instead of the IME pipeline, which works reliably on all platforms. CJK input methods are not supported with this workaround, but that's acceptable for an English-language Elasticsearch tool.

**Do NOT remove this line** — removing it will re-break all text input on Linux.

### Debounced text filter inputs

**Symptom:** Filter text boxes (cluster sidebar filter, indices/datastream filter) previously applied their filter on every keystroke, which caused layout thrashing and made typing feel laggy.

**Fix:** Both filter inputs now use a 300ms debounce (`FILTER_DEBOUNCE_MS` in `src/modules/indices.rs`). The textbox buffer updates instantly for smooth typing, but the actual filter is only applied 300ms after the last keystroke. The debounce decision logic (`filter_debounce_due` / `filter_debounce_remaining`) is extracted as pure, testable functions with unit tests covering no-input, within-window, boundary, past-window, and multi-keystroke-burst scenarios.
