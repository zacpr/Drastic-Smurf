# Changelog

All notable changes to this project will be documented in this file.

## [0.5.17] - 2026-11-05

Version 0.5.16 was not published because Windows MSI validation failed; this
release includes its changes and the installer correction.

### Fixed
- Windows MSI advertised shortcuts use an icon identifier ending in `.exe`, matching the executable key file as required by WiX ICE50 validation.

### Added
- Windows MSI compilation and validation in PR CI to catch installer failures before releasing.

## [0.5.16] - 2026-11-05 (unpublished)

### Added
- Windows MSI creates Start Menu and desktop shortcuts and displays the app icon in Installed Apps; uninstall removes the shortcuts.
- Shared transparent character-head icon for the tray, application window, Windows shortcuts, and Linux launcher.
- Reproducible PNG and multi-resolution Windows ICO generation with `scripts/gen_icons.py`.

### Fixed
- RPM installation on Fedora 45: disabled the unused tray-menu `libxdo` feature, removing the Ubuntu-built binary's unavailable `libxdo.so.3` dependency.
- DEB and RPM packages install the application icon in the hicolor icon theme instead of using a generic launcher icon.

## [0.5.15] - 2026-11-04

### Added
- Osquery targeting like Kibana's: debounced type-to-search agent picker (scales to thousands of agents), a Groups panel (all agents, platforms, Osquery policies, with counts), and a filterable picker for Kibana Osquery saved queries.
- Console and God-mode Console presets fill in request and response examples from Elastic's API docs under `####### EXAMPLE n` markers (the preset's own body is Example 0), with Keep one / Remove / Show examples controls and a prompt if examples remain when sending.
- Preset hover cards link to each API's page in Elastic's online reference.
- AI Assistant can act in the app through tool calling: list clusters, query them, read the Console, load requests into the Console and switch tabs. Read-only requests run automatically (configurable); anything that can change a cluster waits for approval in the chat.
- AI Assistant shows reasoning models' thinking in a collapsible section.
- `scripts/gen_api_examples.py` regenerates the embedded API examples and doc links from Elastic's OpenAPI specs.

### Fixed
- AI Assistant often showed no reply: every request ended with an empty assistant turn (and failed replies stayed in the history as blanks), which made some models stop immediately. Empty turns are no longer sent, blank replies are explained instead of saved, mid-stream provider errors are shown, long generations are no longer cut off at 120 s, and replies are saved when they finish.
- Pages whose content grows past the window can now be scrolled.
- The tab bar's horizontal scroll bar no longer covers the tab labels.
- "Run SLM Policy Now" preset used POST; the API is PUT.

### Changed
- World clock "Sydney (APAC)" renamed "APAC (Sydney)" to match the others (existing configs are migrated).

## [0.5.13] - 2026-11-01

### Added
- Expanded Elastic Console presets with collapsible categories and title filtering, including Fleet, Kibana data views, alerting, and troubleshooting requests.
- Index Templates and Components explorer tabs with sorting, filtering, system-entity visibility controls, and linked detail dialogs.
- Elasticsearch client support for listing and retrieving index and component templates.
- Regression tests for console presets, filtering, layout, and template exploration.

## [0.5.12] - 2026-10-09

### Fixed
- Linux CI and release builds install GTK 3, Ayatana AppIndicator, and libxdo development dependencies required by tray support.
- x86_64 DEB/RPM packaging reuses the cargo-dist binary and reports packaging failures instead of silently ignoring missing outputs.
- DEB/RPM packages install the dynamically loaded Ayatana AppIndicator runtime library.
- VS Code Linux build tasks invoke Cargo directly instead of requiring `flatpak-spawn`.

## [0.1.8] — 2026-05-16

### Added
- **Ingest Pipeline Simulator** — New "Pipeline" tab for building and testing Elasticsearch-style ingest pipelines. Supports 8 processor types (set, remove, json, reroute, convert, lowercase, uppercase, trim) with step-by-step trace output showing before/after per processor.
- **Pipeline Engine** — Pure Rust engine ported from the TypeScript reference implementation. Path splitting with dot notation, bracket notation, and quoted/escaped keys. Deep diff for change tracking. Full test coverage.
- **Debounced Config Saves** — Config writes are batched on a 5-second timer instead of rewriting JSON on every refresh tick. Force-saves on app exit.
- **Reduce Motion Toggle** — Accessibility option in Appearance tab that disables shimmer, hover glow animations, and background VFX animation to save battery.
- **Window State Persistence** — App remembers and restores window size and position across launches.
- **Error Toast System** — Bottom-right toast notifications surface previously silent failures (save errors, auth errors, cluster CRUD failures).
- **Global Cluster Filter** — Sidebar search input filters clusters across all modules (Snapshot, Status, Tasks, Console).

## [0.1.7] — 2026-05-16

### Added
- **Runtime Theme System** — 8 color presets (Slate, Dracula, Nord, Solarized Dark, Tokyo Night, Monokai, Catppuccin Mocha, One Dark) with per-color customization.
- **Appearance Tab** — New settings tab for selecting themes, adjusting individual colors, and configuring visual effects.
- **Background VFX** — Animated background effects (Gradient and Mesh) with configurable intensity and speed.
- **Animated Widgets** — Shimmer effect on snapshot progress bars and hover glow on status cards, gated by VFX settings.
- **Live Theme Preview** — Appearance tab shows a real-time preview card reflecting the active color scheme.
- **Theme Persistence** — Active theme and VFX settings are saved to `AppConfig` and restored on launch.

### Changed
- `Theme` color constants now resolve dynamically via a thread-local `ACTIVE_THEME` accessor, enabling runtime theme switching without module refactors.
- `ClusterManager` now stores and persists `AppTheme` and `VfxSettings` alongside cluster configuration.

## [0.1.6] — 2026-05-15

### Added
- **Clusters Tab** — Full cluster management UI with add, edit, test connection, and delete.
- **Import / Export** — JSON import/export for cluster configurations with optional inclusion of cached module data.
- **Persistent Caching** — Per-cluster data cache for saved queries, status history, tasks, and snapshots.
- **Console Saved Queries** — Save and reuse frequent Elasticsearch API requests per cluster.
- **Cross-platform Keyring** — Secure password storage via OS-native credential managers (Windows Credential Manager, macOS Keychain, Linux secret-service).
- **Linux `.desktop` Launcher** — App appears in Linux desktop menus when installed via DEB/RPM.
- **DEB/RPM Packaging** — GitHub Actions workflow builds `.deb` and `.rpm` packages on release.

### Fixed
- RPM build now runs explicit `cargo build --release` before packaging.
- DEB/RPM uploads use `gh release upload` to avoid tag requirement issues on manual dispatch.
