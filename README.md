# DRASTIC SMURF

A fast, native desktop GUI for monitoring and interacting with multiple Elasticsearch clusters.

![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## Features

| Module | What it does |
|--------|-------------|
| **Clusters** | Centralized cluster management — add, edit, test connections, import/export config |
| **Snapshot Monitoring** | Live snapshot progress with speed tracking, ETA, sparklines, and SLM policy status |
| **Cluster Status** | At-a-glance health dashboard — nodes, shards, indices, docs, JVM heap |
| **Task Monitoring** | Watch reindex, snapshot, and other cluster tasks in real time with filtering |
| **Elastic Console** | Send raw API requests with saved queries, without re-entering credentials. Picking a preset also fills in request and response examples from Elastic's API docs, under `####### EXAMPLE` marker lines (the preset's own body is Example 0). Keep one example, remove them all, or show them again from the body toolbar; you're prompted if any remain when you send |
| **God-mode Console** | Run one request across many selected clusters in turn, optionally looping over a list of values via `{{loop_item}}`, or chaining one level by extracting JSON paths from every response (one `*` wildcard, `&&` for multiple columns, e.g. `*&&*.settings.index.lifecycle.name`) into a list fed back via `{{results_item}}` / `{{results_item.N}}`, with labelled per-cluster/per-value results, dry run and write confirmation |
| **Osquery** | Run live Osquery queries on Fleet agents found by type-to-search, or on groups (all agents, platforms, agent policies), optionally starting from a Kibana saved query, and inspect their status and results |
| **AI Assistant** | Chat with an OpenAI-compatible model, review saved conversations, and request cluster lookups or app actions with approval required for writes |

- **Multi-cluster** — manage separate credentials and auth per cluster
- **Secure** — passwords and API keys stored in your OS keyring, never in exported configs
- **Persistent** — module data (queries, history, cache) survives restarts
- **Import / Export** — move your cluster config between machines via JSON
- **SSH Tunnels** — connect through jump hosts automatically
- **Dark theme** — easy on the eyes for long monitoring sessions

---

## Installation

### Download a release

Grab a pre-built binary from the [Releases](https://github.com/zacpr/a_drastic_smurf/releases) page:

| Platform | Package |
|----------|---------|
| Linux (x86_64) | `.deb`, `.rpm`, or `.tar.gz` |
| macOS (Intel / Apple Silicon) | `.tar.gz` |
| Windows (x86_64) | `.msi` or `.zip` |

The Windows MSI creates Start Menu and desktop shortcuts named **Drastic Smurf**
and displays the app icon in Installed Apps. Uninstalling removes both shortcuts.
The ZIP download does not create shortcuts.
The tray, application window, Windows shortcuts, and Linux launcher use a
cropped character icon. DEB/RPM packages install the Linux launcher icon.

### Build from source

```bash
# Clone
git clone https://github.com/zacpr/a_drastic_smurf.git
cd a_drastic_smurf

# Build release binary
cargo build --release

# Run
./target/release/drastic-smurf
```

**Linux build deps**
```bash
# Debian/Ubuntu
sudo apt-get install build-essential pkg-config libdbus-1-dev libwayland-dev libxkbcommon-dev libgtk-3-dev libayatana-appindicator3-dev

# Fedora
sudo dnf install gcc gcc-c++ pkgconf-pkg-config dbus-devel wayland-devel libxkbcommon-devel gtk3-devel libayatana-appindicator-gtk3-devel
```

Install a current stable Rust toolchain before building. The VS Code Linux build
tasks run `cargo` directly and do not require `flatpak-spawn`.

---

## Quick Start

1. **Launch the app**
2. Go to the **Clusters** tab and click **Add Cluster**
3. Fill in:
   - **Name** — e.g. `Production`
   - **Host** — e.g. `https://elastic.example.com:9200`
   - **Authentication** — choose **Username / Password** (enter both) or **API Key**
     (paste the base64 `encoded` key from Kibana, or a raw `id:api_key` pair)
   - *(Optional)* **Snapshot Repo**, **SLM Policy** and **Kibana Host**

   Required fields (Name, Host, Username for password auth, and the password or API key)
   are marked in the form; everything else is labelled *Optional*.
4. Click **Test Connection**, then **Save**
5. Switch between tabs to monitor snapshots, status, tasks, or open the console

Clusters are saved to `~/.config/drastic-smurf/config.json`. Passwords and API keys are stored in your OS keyring, not in this file or exports. If the keyring is unavailable, credentials remain in memory for the current session only. Module data (saved queries, status history, tasks cache, snapshot cache) is stored alongside the config.

API-key authentication applies to Elasticsearch and the cluster's Kibana host. The key must have the privileges needed by the modules you use, including cluster monitoring for the connection health test. Existing configurations continue to use username/password authentication.

### AI Assistant

Open the assistant's settings, choose a provider and model, set the API key when required, and enable the assistant. The first message creates a conversation automatically; use the conversation picker to return to earlier chats.

**App access** controls tool calling. A compatible model can list clusters, inspect the Console, fetch cluster data, load a request into the Console without sending it, or switch tabs when asked. GET/HEAD requests can run automatically (configurable in settings); other methods always require approval. Models without tool support can still chat.

Cancel, starting a new chat, switching chats, or deleting the active chat stops the reply and discards queued actions. Requests already sent may still complete. Conversation history and tool results are saved in `config.json`; cluster context and approved tool results are sent to the selected provider.

### SSH Connections

SSH tunnels open on demand and reconnect after SSH exits. Configure key-based authentication and host trust in OpenSSH; the app cannot type an SSH password into a terminal prompt. The cluster URL's hostname and port must be reachable from the SSH server. **Test Connection** checks both tunnel startup and Elasticsearch health and shows SSH logs.

Hostname-based HTTPS tunnels preserve SNI, the original Host header, and certificate verification. IP-address targets bypass TLS certificate checks; prefer a hostname matching the certificate. A separate Kibana host is not forwarded by the Elasticsearch tunnel.

For Osquery, configure the cluster's Kibana host and install the Osquery Manager
integration on the target Fleet agents. The Kibana account needs Fleet agent
and agent policy read access (for search and the platform/policy groups) and
Osquery Manager privileges to run queries, read saved queries, and view results;
older Elastic Stack versions may also require index privileges. See Elastic's
[Osquery prerequisites](https://www.elastic.co/docs/solutions/security/investigate/osquery#required-privileges).
The tab uses Kibana's versioned live-query API, available in Kibana 8.19 and 9.x.

ES|QL queries can be run in Elastic Console using the **Search & Analytics >
ES|QL Query** preset. ES|QL requires a compatible Elasticsearch version and
read access to the queried indices.

---

## Tech Stack

- **Rust** + **egui** — immediate-mode GUI, native performance
- **tokio** — async HTTP requests to all clusters concurrently
- **reqwest** — TLS, custom CA certs, SSL verification toggle
- **keyring** — OS-native secret storage

---

## Development

Regenerate the shared icon and Windows ICO from the existing artwork with
`python scripts/gen_icons.py` (requires Pillow). The full artwork inside the
app is unchanged.

```bash
# Check
cargo check --all-targets

# Run debug build
cargo run

# Build packages (requires release binary)
cargo install --locked cargo-deb cargo-generate-rpm
cargo deb          # Debian/Ubuntu
cargo generate-rpm # RHEL/Fedora
```

### Linux Graphics

A local debug run crashed in Mesa's threaded OpenGL renderer. If this occurs,
disable threaded OpenGL for that process only:

```bash
mesa_glthread=false ./target/debug/drastic-smurf
```

The app started successfully with this workaround; long-term stability is not
yet verified. It does not change system graphics settings.

Version-tag releases build x86_64 DEB/RPM packages in GitHub Actions and attach
them to the GitHub Release. A missing package fails the release rather than
silently publishing without it.
Both packages declare the dynamically loaded Ayatana AppIndicator runtime
dependency so tray support also works on a fresh installation.
The tray's optional `libxdo` feature is disabled: it only implements predefined
edit-menu actions that the app does not use. This avoids a `libxdo.so.3`
requirement that cannot be resolved on Fedora 45, which provides `libxdo.so.4`.
The tray's Show Window/Quit actions and egui clipboard support are unaffected.

---

## License

MIT — see [LICENSE](LICENSE) for details.
