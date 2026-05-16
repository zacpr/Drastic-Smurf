# Changelog

All notable changes to this project will be documented in this file.

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
