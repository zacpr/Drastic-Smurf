# DRASTIC SMURF

A fast, native desktop GUI for monitoring and interacting with multiple Elasticsearch clusters.

![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## Features

| Module | What it does |
|--------|-------------|
| **Snapshot Monitoring** | Live snapshot progress with speed tracking, ETA, sparklines, and SLM policy status |
| **Cluster Status** | At-a-glance health dashboard — nodes, shards, indices, docs, JVM heap |
| **Task Monitoring** | Watch reindex, snapshot, and other cluster tasks in real time with filtering |
| **Elastic Console** | Send raw API requests to any configured cluster without re-entering credentials |

- **Multi-cluster** — manage separate credentials and auth per cluster
- **Secure** — passwords stored in your OS keyring, never on disk
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
sudo apt-get install libwayland-dev libxkbcommon-dev   # Debian/Ubuntu
```

---

## Quick Start

1. **Launch the app**
2. Click **+ Add Cluster** in the sidebar
3. Fill in:
   - **Name** — e.g. `Production`
   - **Host** — e.g. `https://elastic.example.com:9200`
   - **Username** & **Password**
   - *(Optional)* **Snapshot Repo** and **SLM Policy** for snapshot monitoring
4. Click **Test Connection**, then **Save**
5. Switch between tabs to monitor snapshots, status, tasks, or open the console

Clusters are saved to `~/.config/drastic-smurf/config.json` and passwords are stored in your OS keyring.

---

## Tech Stack

- **Rust** + **egui** — immediate-mode GUI, native performance
- **tokio** — async HTTP requests to all clusters concurrently
- **reqwest** — TLS, custom CA certs, SSL verification toggle
- **keyring** — OS-native secret storage

---

## Development

```bash
# Check
cargo check --all-targets

# Run debug build
cargo run

# Build packages (requires release binary)
cargo deb          # Debian/Ubuntu
cargo generate-rpm # RHEL/Fedora
```

---

## License

MIT — see [LICENSE](LICENSE) for details.
