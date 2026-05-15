# AGENTS.md — DRASTIC SMURF

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
| **Task Monitoring** | ✅ Functional | Filterable task grid (cluster, action, description, running time, cancellable). |
| **Elastic Console** | ✅ Functional | Method/path/body request builder with response viewer. Inherits cluster credentials. |

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
│   ├── mod.rs
│   └── ssh_tunnel.rs   # SSH tunnel spawning via `ssh -L`
├── modules/
│   ├── console.rs      # Elastic Console tab
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

- **Unit tests** — planned for JSON parsing, stat translation, utility functions
- **Integration tests** — planned against a local Elasticsearch instance or mock HTTP server
- **UI tests** — limited; egui does not have a built-in UI testing framework

*(No tests are currently implemented — this is a known gap.)*

---

## Security Considerations

- **Do not commit credentials.** Passwords and API keys are stored in the OS keyring.
- Use `directories` crate for config storage (`~/.config/drastic-smurf/` on Linux).
- TLS verification is on by default; per-cluster override available.
- Custom CA certificate support is partially implemented (`CaCert::Custom` works; `CaCert::Bundled` is a TODO).
- API token auth methods are stubbed in `auth.rs` but not yet wired into `EsClient`.

---

## CI / Release

### GitHub Actions Workflows

| Workflow | Trigger | What it does |
|----------|---------|--------------|
| `ci.yml` | Push/PR to `main`/`master` | `cargo check`, `cargo test`, `cargo fmt --check`, `cargo clippy` |
| `release.yml` | Tag push (`v*.*.*`) | cargo-dist builds archives + MSI + shell/PowerShell installers, creates GitHub Release |
| `packages.yml` | Release published | Builds `.deb` (cargo-deb) and `.rpm` (cargo-generate-rpm), uploads to release |

### Releasing

1. Bump version in `Cargo.toml` (and `Cargo.lock` via `cargo check`)
2. Commit and tag: `git tag v0.x.y`
3. Push tag: `git push origin v0.x.y`
4. cargo-dist creates the release automatically; `packages.yml` appends `.deb` and `.rpm`

---

## Next Steps / Known Gaps

1. **Tests** — Add unit tests for `human_bytes`, `human_duration`, snapshot stat calculations, and config roundtrips.
2. **Status module depth** — Currently shows a flat card list. The plan calls for an overview of all clusters, selected subset view, and detailed single-cluster view.
3. **Task type filtering** — Text search exists, but task-type dropdown filtering is not implemented.
4. **Console enhancements** — No request history navigation, JSON syntax highlighting, or response folding.
5. **AGENTS.md upkeep** — Update this file whenever modules, build steps, or security boundaries change.
