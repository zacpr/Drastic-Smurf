***The Plan***
The goal is the creation of an extensible tool for interacting with and monitoring elasticsearch.
The app msut support multiple clusters with separate credentials and potentially differnt auth methods
so the overarching app will maintain the cluster information, and authentication and privide a tabbed interface for interacting with the vrious modules.

**Expected Modules**
- Snapshot monitoring; this should replicate (or port over etc) the functionality available in /home/zac/app_dev/es-snap-mon/
- Cluster task monitoring; this will mainly provide a way to monitor reindex operations,but shuould provide a way to view oth3er task types , with filtering per type etc
- Cluster status monitoring; this should provide status and health information in a dashboaard formqt. it shoudl be able to provide an overview of all clustrers , a select subset of clusters,  or a more detailed single cluster view
- Elastic/Kibana console; this should provide fucntionaluty similar to the devtools console in kibana , allowing the user to interact withthe api without re-entering credentials or clsuter details


Technical; performqance and responisveness is key, if feasible the project shoudl be written in rust

---

# UI retrofit: egui → Slint (2026-07-12 handover)

Agreed direction plus open questions from a design conversation. Nothing below is committed code — it's a plan to implement on the `newgui` branch. Mockups referenced were interactive HTML/CSS prototypes used to pressure-test layout and information architecture; they don't exist as files, only as descriptions here.

## Why we're moving off egui

egui doesn't handle content overflow, scrollbar necessity, or general layout flow well — it's immediate-mode and largely leaves sizing/clipping to the developer, which is why the current app feels janky despite being functionally solid.

## Why Slint (over Iced, Dioxus, Xilem, GPUI)

- Real layout engine — grid and flexbox primitives, not manual positioning. This directly addresses the overflow/scrolling complaint.
- GPU-accelerated rendering (Skia/wgpu/femtovg backends depending on platform).
- Genuine design-to-code story: an official Figma-to-Slint plugin plus a Live-Preview tool (VS Code extension) for editing `.slint` markup and previewing without full recompiles.
- Royalty-free license covers desktop apps at no cost (only embedded/device use triggers a fee) — not a licensing concern for this use case.
- Dioxus was ruled out specifically because it wraps a WebView — the ask was to avoid Electron-style approaches. Iced was the honest runner-up (pure Rust, no DSL) but has a less mature layout system and no equivalent design-import tooling. Xilem/GPUI aren't production-ready options right now.

## Current app pain points (from screenshots reviewed)

- 11 flat top-level tabs (Clusters, Snapshot, Status, Dashboard, Tasks, Console, Discover, Indices, Observability, Pipeline Simulator, Settings) — too many parallel concepts, no grouping.
- Left sidebar stacks three unrelated concerns (cluster list, world clocks, refresh settings) with no separation — falls apart at short window heights.
- "New Cluster" form is a flat, ungrouped list of ~10 fields (name/host/auth, integrations, security, import) all at equal visual weight.
- Console's command list has no collapse/scroll affordance — no indication there's more below the fold.
- Console's request/response panels only use a fraction of available vertical space.

## Agreed redesign direction

### Persistent cross-cluster alert bar

Sits above everything, visible regardless of which screen or cluster is active — this was a hard requirement, not a nice-to-have.

- **Quiet state**: when no cluster is currently alerting, it collapses to a single flat, calm strip ("all clusters healthy") with no expand affordance — there's nothing to disclose.
- **Alert state**: expands to show a mascot avatar (placeholder for now, see below) beside a list containing **only the clusters currently contributing an active alert** — healthy or filtered-out clusters are not listed, even greyed out. If a cluster's home lab connection is down from the office/VPN, that shouldn't visually compete with a genuine cluster health problem.
- This bar is independent of the sidebar's cluster filter (see below) — it always considers all configured clusters regardless of what's currently "focused," since it's meant to be a safety net.

### Per-cluster, configurable alert conditions (Settings)

Originally hardcoded to "fires on status red only," now needs to be a toggle list per cluster, because different people's monitoring setups differ (e.g. someone without separate infra monitoring may want "cluster offline" to alert; someone with existing ping monitoring wouldn't).

Proposed condition set, each with independent default:

- Cluster status red — **default on**
- Cluster status yellow — default off
- Cluster unreachable / offline — default off (assumption: something else monitors dead hosts)
- Authentication or TLS failure — **default on**
- Disk watermark breached (high/flood stage) — **default on**
- No master node elected — **default on**
- Sustained JVM heap pressure (e.g. >85% for several minutes) — default off

UI pattern agreed: Settings → Clusters → [cluster] → Alerting panel, with a master "follow global defaults" toggle at the top. Off = customize just this cluster; on = inherits from a global defaults page (that global page itself wasn't designed in detail — open item, see below).

### Mascot

There's a "DRASTIC SMURF" branded mascot image the user has (a stressed/frazzled pose) they want reflected in the alert bar — calm state, an intermediate yellow/degraded state, and the red/stressed state shown when things are actively wrong. No artwork has been generated for this — it's a copyrighted/trademarked character design (Smurfs, Peyo/IMPS), and that's not something to reproduce even for an internal tool. The mockups use a generic Tabler mood-icon placeholder with a colored ring that swaps per state, structured so swapping to real `<image>` assets per status is a one-line change once the user supplies three actual image files. An original (non-Smurf) mascot concept is fair game if wanted instead.

### Sidebar navigation

- Top: **Dashboard** (home/landing item, not nested in a group).
- Below that: a **Focused clusters** section — collapsible (starts collapsed to a summary line like "Focused clusters (4 of 6)"), with a name-filter text input plus **checkboxes per cluster** (not just text search) so the user can pin a working subset. Needs its own internal scroll region with a fixed max-height, since real usage is 8 clusters now, growing toward 12–15 — it must not dominate the sidebar at that scale.
- Unchecking a cluster here doesn't remove it from config — it just narrows what's "in scope" across Home and any active-cluster picker (see below).
- Grouped nav below: **Monitor** (Status, Observability), **Operate** (Tasks, Snapshot), **Explore** (Discover, Indices, Console, Pipeline Simulator).
- Bottom: a small icon strip (world clocks, settings). No "+Add Cluster" button in the sidebar anymore.

### Cluster administration moves to Settings

Add/Edit/Import/Export/Test cluster all belong in Settings now, not on a general-purpose "Clusters" screen. When zero clusters are configured, the app should direct the user to a setup/quickstart flow — **this empty state was not designed**, just agreed as a requirement.

### Active-cluster selector (per-screen pattern)

Every screen except Dashboard-overview and Settings is inherently single-cluster-scoped (Console, Status, Tasks, Snapshot, Discover, Indices, Observability, Pipeline Simulator). Each needs a consistent header row: breadcrumb on the left, an "Active cluster" dropdown on the right, **populated only from the currently focused/checked cluster set** (ties back into the sidebar filter). This was only actually built out for Console in the mockup — the same header pattern needs to repeat identically on the other single-cluster screens.

### Dashboard (home screen)

This is the first-impression screen, so it got the most attention. Two modes via a sub-tab pair:

**Overview grid** — a card per focused cluster: name, connection URL, status badge (green/amber/red/offline styling), and for reachable clusters a row of key stats (nodes, shards, size) plus a JVM heap usage bar. Offline clusters just show the error inline instead. When clusters are hidden via the sidebar filter, a small "N hidden — see focused clusters" line appears above the grid, clicking it jumps to and highlights the filter section.

**Detailed single cluster** — its own active-cluster dropdown (same focused-subset pattern as above) plus:

- Header card: cluster name, status badge, and a row of link chips for whichever URLs are configured — **API link always present** (it's how the tool connects in the first place), Kibana/HAProxy/custom links only rendered if the user configured them for that cluster.
- Four metric cards: total nodes, active/primary shards, documents, store size.
- Node list table: name, role, CPU/RAM/heap as inline usage bars, status (with a distinct "Master" badge).
- Shard allocation/disk table: node, shard count, disk free/total, disk usage as a bar.
- If the selected cluster is offline, live-data sections clear out and a plain "no live data — links above still work" message takes their place, rather than showing empty tables.

### Usage bars (CPU/RAM/heap/disk)

Important implementation detail, not just cosmetic: these should be a **fixed-position gradient (green → amber → red, left to right) with a mask overlay revealing only up to the actual percentage** — not a gradient stretched to fit the bar's own rendered width. The stretched approach is the common bug: a nearly-empty bar can show a sliver of red at its tip purely because the whole spectrum got compressed into a short width, which is misleading. Get the semantics right: low usage should read as calm/green regardless of how short the bar is.

### Console

- Workspace switched from buried sub-tabs to first-class tabs: **Presets / Custom / Variables / History**.
- Presets are a collapsible tree (grouped by category — cluster status, nodes/allocation, indices/templates, search/analytics, ingest/ILM) using native disclosure semantics, with a properly scrollable, visibly-scrollbarred container instead of an undifferentiated flat list.
- **Custom** is where user-saved commands should live, kept separate from built-in presets — data model/UX for saving a command wasn't designed, just the tab slot.
- **Variables**: key/value pairs, substitutable into any command across any cluster — this already exists in the current app (under a buried "Vars" tab) and needed to be preserved and promoted, not redesigned from scratch.
- **History** tab exists as a placeholder only — not designed.
- Request/response panels should fill available vertical height rather than floating in mostly-empty space.

## Suggested build order

1. Get Slint building at all — `cargo add slint`, `slint-build` as a build dependency, a `ui/` directory for `.slint` files wired through `build.rs`. Confirm the Live-Preview VS Code extension works for fast iteration.
2. Audit how entangled the current cluster/ES-client/config logic is with the egui `update()` loop. If it's already reasonably separable, the Slint UI can be built alongside the old one; if not, decoupling that logic is the real first task, more important than the UI toolkit choice.
3. Build the persistent shell first (alert bar, sidebar nav, cluster filter) bound to static/dummy data. This is the best early test of whether Slint's layout actually fixes the overflow/scrolling complaints against egui — worth confirming before investing further.
4. Wire the shell to real data.
5. Build Dashboard (overview grid) as the first real content screen — self-contained, highest first-impression value.
6. Console next — meaningfully harder (tree, tabs, resizable-feeling panes) and better attempted once there's a feel for Slint's layout primitives from Dashboard.
7. Continue screen by screen, applying the same active-cluster-selector and general layout patterns established above.

## Open questions / not yet decided

- **Coexistence strategy**: run old (egui) and new (Slint) UI as separate binaries sharing a core crate during migration, or work on a branch and cut over screen by screen with the app in a half-migrated state? Flagged as a decision that affects crate structure — unanswered as of this handover.
- Empty-state / quickstart flow when zero clusters are configured — not designed.
- Full Settings screen layout (cluster CRUD forms, global Alerting-defaults page) — only the per-cluster Alerting toggle panel was mocked in isolation.
- Mascot: needs three actual image assets from the user (calm / yellow-alert / red-alert) before the placeholder can be replaced.
- Console's Custom-commands and History tabs need actual design, not just tab slots.
