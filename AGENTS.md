# AGENTS.md — Elasticsearch Multi-Tool (DRASTIC SMURF)

## Project Overview

**Status:** Planning phase — no source code, build system, or directory structure exists yet.  
**Goal:** Build an extensible desktop GUI for interacting with and monitoring multiple Elasticsearch clusters. The app must support multiple clusters with separate credentials and potentially different authentication methods. It will maintain cluster information and authentication centrally, and expose a tabbed interface for interacting with various modules.

The project directory currently contains only `theplan.md`, which describes the intended feature set and module breakdown.

### Expected Modules

| Module | Description |
|--------|-------------|
| **Snapshot Monitoring** | Replicate or port the functionality from `/home/zac/app_dev/es-snap-mon/` — track ongoing Elasticsearch snapshot backups across clusters, translate stats into human-readable progress, bytes, ETA, and speed. |
| **Cluster Task Monitoring** | Provide a way to monitor reindex operations and other Elasticsearch task types, with filtering per task type. |
| **Cluster Status Monitoring** | Provide status and health information in a dashboard format. Capable of showing an overview of all clusters, a selected subset, or a more detailed single-cluster view. |
| **Elastic/Kibana Console** | Functionality similar to the Kibana DevTools console, allowing the user to interact with the Elasticsearch API without re-entering credentials or cluster details for each request. |

### Reference Project

A prior implementation of the snapshot monitoring module exists at `/home/zac/app_dev/es-snap-mon/`. It is written in **Python 3.9+** using **CustomTkinter**, **Requests**, and **Keyring**. That codebase can serve as a functional reference for:

- Elasticsearch API endpoints (`_snapshot`, `_slm/policy`, `_cluster/health`, `_status`, etc.)
- JSON response parsing and human-readable stat translation
- Secure credential storage patterns
- Dark-themed dashboard UI patterns (cards, progress bars, sparklines)

**Important:** This new project is *not* a direct continuation of `es-snap-mon`. It is a fresh project with broader scope, a modular tabbed architecture, and a strong preference for a different technology stack.

---

## Technology Stack

**Not finalized yet.** Performance and responsiveness are key requirements. The plan explicitly states:

> "if feasible the project should be written in rust"

Candidate stacks under consideration:

- **Rust** + **egui** / **iced** / **Tauri** — preferred direction due to performance goals.
- **Rust** + **Tauri** (Web frontend in TypeScript) — if a web-based UI is desired.
- **Python** + a modern GUI framework — fallback if Rust feasibility is blocked.

The chosen stack must support:
1. Asynchronous HTTP requests to multiple Elasticsearch clusters concurrently.
2. Secure cross-platform secret storage for passwords and API tokens.
3. A tabbed, modular UI where each module can be developed and loaded independently.
4. JSON parsing and dynamic rendering of Elasticsearch API responses.

---

## Project Layout (Proposed)

Until a stack is chosen, no directory structure exists. A typical layout once development starts might look like:

```
├── src/                 # Application source code
│   ├── core/            # Shared cluster management, auth, and ES client
│   ├── modules/         # Individual feature modules (snapshot, tasks, status, console)
│   └── ui/              # Main window, tab container, shared widgets
├── tests/               # Unit and integration tests
├── docs/                # Additional documentation
├── config/              # Example configuration files
├── Cargo.toml           # or pyproject.toml, package.json, etc.
└── README.md            # Human-facing documentation
```

---

## Build and Test Commands

**Not applicable yet.** Once a stack is selected, add the standard commands here (e.g., `cargo build`, `cargo test`, `pip install -e .`, `npm run build`).

---

## Code Style Guidelines

**Not defined yet.** Decide and document formatting rules once the stack is chosen:

- **Rust:** `rustfmt` + `clippy`
- **Python:** `black` / `ruff`
- **TypeScript:** `prettier` / `eslint`

---

## Testing Instructions

**Not defined yet.** Plan to include:

- **Unit tests** for JSON parsing, stat translation, and utility functions.
- **Integration tests** against a local Elasticsearch instance or a mock HTTP server.
- **UI tests** if the chosen framework supports them.
- **End-to-end tests** for the cluster credential flow and tab switching.

---

## Security Considerations

- **Do not commit credentials.** Cluster passwords, API keys, and certificates must never be hard-coded in source.
- Use OS-native secret storage (Keychain, Windows Credential Manager, libsecret / keyring) for all sensitive credentials.
- Validate TLS certificates when connecting to production clusters. Allow per-cluster overrides only for development/testing.
- Support custom CA bundles for clusters with private certificates.
- Consider least-privilege Elasticsearch users for each module (e.g., `monitor` + `snapshot` roles for snapshot monitoring only).
- The **Elastic/Kibana Console** module will execute arbitrary API calls on behalf of the user — ensure it inherits the same cluster credentials securely and does not log or expose secrets.

---

## Next Steps for Agents

1. **Confirm the technology stack with the user.** Rust is strongly preferred, but feasibility (team expertise, ES client libraries, GUI framework maturity) should be validated.
2. **Initialize the project** with the appropriate package manager and build tool (e.g., `cargo init`, `npm init`, `poetry init`).
3. **Implement the core layer first:**
   - Cluster configuration model (name, host, credentials, SSL settings).
   - Secure credential storage abstraction.
   - Async Elasticsearch HTTP client with basic auth and SSL support.
4. **Build a minimal proof-of-life:** query one cluster's `_cluster/health` endpoint and display the result in the UI.
5. **Port snapshot monitoring** as the first module, using `es-snap-mon` as a reference for API usage and stat calculations.
6. **Add the remaining modules** (task monitoring, cluster status dashboard, console) incrementally behind a tabbed interface.
7. **Add tests and CI** once the core modules are stable.
