# Tephra Client — Product Requirements Document

## Overview

Tephra Client is a GPU-accelerated multi-node CPU thermal monitoring dashboard built in Rust with iced. It connects to one or more `tephra-server` instances over the LAN via SSE, displaying real-time telemetry: temperature, package power, frequency, utilization, fan RPM, per-core breakdowns, throttle events, and workload tracking.

The visual identity is **volcanic instrumentation** — obsidian-dark backgrounds with ember/magma accents. Not a generic dark-theme dashboard.

**Target hardware:** Linux workstations and laptops with AMD/Intel CPUs. Primary dev target is an AMD Ryzen 9 7945HX (32 threads).

**Stack:** Rust 2024, iced 0.14 (wgpu, Canvas), reqwest + reqwest-eventsource (SSE), tokio.

---

## Current State (v0.2)

### What works today

- **Dashboard view** — responsive card grid (1/2/3 columns) with live sparklines, metrics, trend arrows, throttle badges, and notification badges for unread events
- **Detail view** — three tabs:
  - **Overview**: line charts (temp, power, freq, util), optional fan chart, temperature duration curve
  - **Cores**: per-core frequency/utilization heatmap grid with busiest-core highlighting
  - **Events**: throttle log + workload list with auto-scroll, detailed completion stats
- **Workload overlay** — modal with full workload stats, prev/next navigation
- **Workload detection** — client-side workload start/end detection with per-workload throttle event tracking
- **Networking** — SSE streaming with auto-reconnect (exponential backoff, 1s–30s), connection failure after 10 retries with retry button, 5s per-request timeout, graceful disconnect on node removal, subscription deduplication
- **Localhost auto-detection** — probes `127.0.0.1:9867/health` on startup
- **Server version check** — warns when server version is below minimum supported
- **History backfill** — fetches `/api/v1/history` on connect to populate ring buffers (configurable capacity)
- **Derived metrics** — per-core power, efficiency baseline/delta (color-coded), trend computation, sigma, clock ratio (color-coded)
- **Config persistence** — versioned config (v1) with migration support, node list + display names + history capacity in `~/.config/tephra/nodes.toml`
- **Per-node display names** — custom names override hostname, persisted in config
- **Keyboard shortcuts** — Esc, 1/2/3 (tabs), a (add node), w (workload overlay), r (reset stats), p (pause), arrow keys (navigate nodes)
- **Add/remove nodes** — manual IP:port entry with input validation and duplicate check, removal from detail view
- **Pause display** — `p` key freezes display updates while data keeps streaming
- **Throttle flash** — metrics strip and card borders flash on throttle state transitions
- **Active workload indicator** — shows `WORKLOAD #N [Xs]` in utilization pill
- **Selective cache invalidation** — only redraws charts for the currently-viewed node
- **Unit tests** — 34 tests covering API types, ring buffers, chart logic, core data logic, and color thresholds

### Known gaps

- `discovery/` module is a stub — no mDNS
- No TLS, no authentication
- No long-term history persistence (in-memory only, configurable 2–30 min window)
- No error toast / status bar for transient errors (logged to tracing only)
- Server-side peak reset not implemented (`r` key only resets client-side state)

---

## Vision

Tephra Client should feel like a purpose-built volcanic monitoring station — dense, real-time, beautiful, and useful. It should be the kind of tool where you leave it running on a second monitor because it's both informative and satisfying to look at.

**Non-goals:**
- Not a general system monitor (no disk, network, GPU, memory)
- Not a remote management tool (read-only telemetry, not control plane)
- Not cross-platform in the near term (Linux-first, Wayland-first)

---

## Task Backlog

Tasks are grouped by theme and roughly ordered by priority within each group. Size estimates: **S** (< 1 session), **M** (1–2 sessions), **L** (3+ sessions).

### P0 — Stability & Polish

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| P0-1 | Connection failure state | S | ✅ Done | After 10 consecutive reconnect failures, transitions to `ConnectionStatus::Failed`. Retry button on card/detail view. |
| P0-2 | Graceful disconnect on remove | S | ✅ Done | `send!` macro returns when receiver is dropped, cleanly aborting the SSE async task. |
| P0-3 | Error toast / status bar | M | | Surface transient errors (parse failures, unexpected SSE events, config write failures) via a toast or status bar instead of only logging to tracing. |
| P0-4 | Input validation in add-node dialog | S | ✅ Done | Validates IP/hostname format, shows inline error, prevents duplicates. |
| P0-5 | Config migration | S | ✅ Done | Versioned config (v1) with automatic v0→v1 migration on load. |

### P1 — Discovery & Connectivity

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| P1-1 | mDNS auto-discovery | M | | Implement the `discovery/` module. Use `mdns-sd` or `zeroconf` to listen for `_tephra._tcp.local.` services. Show discovered-but-not-added nodes in the add dialog or as ghost cards on the dashboard. Requires server-side mDNS registration first. |
| P1-1a | Localhost auto-detection | S | ✅ Done | Probes `127.0.0.1:9867/health` on startup, auto-adds if a server responds. Skips if already in saved config. |
| P1-1b | Periodic discovery sweep | M | | Run auto-detection on a recurring timer (e.g., every 30s) via an iced `Subscription`, not just on startup. |
| P1-2 | TLS support | S | | Support `https://` connections. The rustls dependency is already present. Add a per-node `tls: bool` config field (or auto-detect via port convention). |
| P1-3 | Connection timeout | S | ✅ Done | 5s per-request timeout on all `fetch_json` calls via `reqwest::RequestBuilder::timeout`. |
| P1-4 | Server version compatibility check | S | ✅ Done | Compares `agent_version` from SystemInfo against minimum, shows warning in detail view if too old. |

### P2 — Data & History

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| P2-1 | Extended ring buffers | S | ✅ Done | `history_capacity` configurable in config (default 240 = 2min, 1200 = 10min, 3600 = 30min). |
| P2-2 | Local history persistence | L | | Write snapshots to a local SQLite/redb database. Allow scrubbing back through historical data in the charts. |
| P2-3 | Data export | M | | Export current session or historical data as CSV or JSON. Triggered via keyboard shortcut or menu. |
| P2-4 | Peak reset (client + server) | S | | Server-side `POST /api/v1/reset-peaks` endpoint needed. Client-side reset already works via `r` key. |

### P3 — UI Enhancements

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| P3-1 | Chart zoom / time range selector | M | | Allow selecting time range on charts (last 1m / 2m / 5m / 10m / all). Requires extended ring buffers (P2-1, done) or local history (P2-2). |
| P3-2 | Node ordering & grouping | S | | Allow drag-to-reorder or manual sort of nodes on the dashboard. Persist order in config. |
| P3-3 | Comparative overlay | M | | Select 2+ nodes and overlay their metrics on a shared chart for side-by-side comparison. |
| P3-4 | Mini/compact dashboard mode | M | | A denser dashboard layout — single-row per node, no sparklines, just key numbers. |
| P3-5 | Animation & transitions | M | | Smooth transitions between dashboard ↔ detail views. Fade-in for new data points on charts. |
| P3-6 | Temperature heatmap timeline | M | | A horizontal heatmap strip showing temperature over time, color-coded by volcanic palette. |
| P3-7 | Notification badges on dashboard | S | ✅ Done | Orange badge count on node cards for unread throttle events/workloads since last detail view visit. |

### P4 — Configuration & Settings

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| P4-1 | Settings view | M | | In-app settings screen for: polling interval preference, chart time range, theme variant, node display names/aliases. |
| P4-2 | Per-node display names | S | ✅ Done | `custom_name` field overrides hostname. Persisted in config `[display_names]` table. `SetDisplayName` message wired up. |
| P4-3 | Alert thresholds | M | | User-configurable temperature/power thresholds with visual alerts. |
| P4-4 | Keyboard shortcut customization | S | | Allow rebinding keyboard shortcuts via config file. |

### P5 — Performance & Architecture

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| P5-1 | Selective cache invalidation | S | ✅ Done | Only clears chart caches for the currently-viewed detail node. Sparkline always refreshed. Full clear on view entry. |
| P5-2 | Subscription deduplication | S | ✅ Done | NodeId included in recipe hash. Old subscription aborts cleanly via `send!` error detection (P0-2). |
| P5-3 | Lazy detail view rendering | S | | Only subscribe to / process detailed rendering for the currently-viewed node. |
| P5-4 | Benchmark harness | M | | A mock tephra-server that replays recorded SSE data at 2Hz. |

### P6 — Future / Exploratory

| ID | Task | Size | Description |
|----|------|------|-------------|
| P6-1 | System tray / background mode | M | Minimize to system tray. Show a notification when a node crosses a thermal threshold. Requires a tray icon library compatible with Wayland. |
| P6-2 | Server fleet management view | L | For users with many (10+) nodes: sortable table view, aggregate stats (hottest node, total fleet power), fleet-wide thermal heatmap. |
| P6-3 | Plugin / extension system | L | Allow custom metric panels or data sources. Very exploratory — only worth considering if the core product is stable and there's demand. |
| P6-4 | Mobile companion | L | A lightweight web UI or mobile app that connects to the same tephra-server instances. Would require either a tephra-client API proxy or direct SSE from mobile. Very far out. |

### P1.5 — Reference Feature Parity

Features from cpu_thermal_monitor that are not yet implemented.

| ID | Task | Size | Needs Server? | Status | Description |
|----|------|------|---------------|--------|-------------|
| RF-1 | Pause display (`p` key) | S | No | ✅ Done | `p` toggles pause. LAVA "PAUSED" badge shown. Data streams but display frozen. |
| RF-2 | Reset peaks (`r` key) | S | Partial | ✅ Client done | `r` resets client-side state. Server-side `POST /api/v1/reset-peaks` still needed. |
| RF-3 | Throttle flash animation | S | No | ✅ Done | `throttle_changed_at: Instant` tracked. LAVA border flash for 2s on transitions. |
| RF-4 | Efficiency delta color coding | S | No | ✅ Done | Freq pill accent: MAGMA if < -5%, GEOTHERMAL if > +5%, MINERAL otherwise. |
| RF-5 | Session summary on quit | M | No | | On app close, print session summary to stdout. |
| RF-6 | Report/profile JSON export | M | Partial | | Generate structured JSON report and save to `~/cpu_thermal_reports/`. |
| RF-7 | Stress test triggering | L | Yes | | Requires server-side stress endpoints. |
| RF-8 | Report comparison view | M | Partial | | Load saved JSON reports for side-by-side comparison. Depends on RF-6. |
| RF-9 | Workload cooldown shadow accumulator | M | No | | Track cooldown data separately so finalized averages exclude the tail. |
| RF-10 | Workload overlay missing fields | S | Yes | | Show additional workload stats. Requires server-side `WorkloadEndEvent` fields. |
| RF-11 | Per-workload throttle event tracking | S | No | ✅ Done | Throttle transitions during active workloads tracked and reported in `WorkloadEndEvent`. |
| RF-12 | Compact/full mode toggle (`c` key) | M | No | ✅ Done | `c` toggles compact mode. Hides fan chart and temp duration curve in overview. COMPACT badge shown. |
| RF-13 | Active workload indicator in metrics strip | S | No | ✅ Done | Shows `WORKLOAD #N [Xs]` in utilization pill subtitle when workload active. |
| RF-14 | Clock ratio in frequency pill | S | No | ✅ Done | `XX% of max` in freq subtitle. Color: GEOTHERMAL >85%, LAVA >50%, MAGMA below. |
| RF-15 | Agent version display | S | No | ✅ Done | `agent_version` shown in system info line. Version warning if below minimum. |
| RF-16 | Event log auto-scroll | S | No | ✅ Done | Throttle and workload columns use `scrollable::anchor_bottom()`. |

### P1.6 — Bug Fixes

All bug fixes completed.

| ID | Task | Size | Status | Description |
|----|------|------|--------|-------------|
| BF-1 | `r` key is a dead stub | S | ✅ Done | `r` key calls `node.reset_client_state()` in detail view. |
| BF-2 | Client workload throttle events always 0 | S | ✅ Done | Throttle transitions tracked per-workload via `workload_thermal_events`/`workload_power_events`. |
| BF-3 | Efficiency delta not color-coded | S | ✅ Done | Freq pill accent color changes based on efficiency delta (see RF-4). |
| BF-4 | `OpenWorkloadOverlay` message variant unused | S | ✅ Done | `w` key now dispatches `Message::OpenWorkloadOverlay` instead of inline logic. |

### T0 — Unit Tests: Core Data Logic (`node/mod.rs`)

All 13 tests implemented. Tests live in `node/mod.rs` `#[cfg(test)]` module.

| ID | Test | Status | Description |
|----|------|--------|-------------|
| T0-1 | Throttle tracking | ✅ Done | `throttle_ticks` increments only when `throttle_active`, `throttle_secs()` converts correctly |
| T0-2 | Temp duration at-exactly | ✅ Done | `temp_duration_ticks[t]` increments only for the exact degree |
| T0-3 | Temp streak at-or-above | ✅ Done | `temp_streak_current[t]` increments for all `t <= temp_c`, resets for `t > temp_c`, `temp_streak_max[t]` tracks longest |
| T0-4 | Cumulative temp secs | ✅ Done | `cumulative_temp_secs(t)` sums `temp_duration_ticks[t..=105]` and converts to seconds |
| T0-5 | Efficiency baseline capture | ✅ Done | Captured on first snap where `ppt_watts > 5.0`, not overwritten on subsequent snaps |
| T0-6 | Efficiency delta computation | ✅ Done | Correct % change from baseline, returns None when power < 0.1 or no baseline |
| T0-7 | Per-core power | ✅ Done | `busy_core_count` uses 20% threshold, `per_core_power` divides PPT by busy count |
| T0-8 | Workload start detection | ✅ Done | Starts after 10 consecutive ticks of avg_util ≥ 25%, counter resets on drops below |
| T0-9 | Workload end detection | ✅ Done | Ends after 10 consecutive ticks of avg_util < 15%, counter resets on recovery |
| T0-10 | Workload stats accumulation | ✅ Done | avg/peak temp, avg/peak power, energy delta, avg freq, avg util computed correctly in `WorkloadEndEvent` |
| T0-11 | Workload ID incrementing | ✅ Done | Sequential workloads get incrementing IDs |
| T0-12 | Server workload events | ✅ Done | `on_workload_start` sets active, `on_workload_end` clears and appends to completed |
| T0-13 | Display name | ✅ Done | Uses custom_name first, then hostname from SystemInfo, falls back to addr |

### T1 — Unit Tests: History & Trend (`node/history.rs`)

| ID | Test | Status | Description |
|----|------|--------|-------------|
| T1-1 | `compute_trend` stable on insufficient data | ✅ Done | < 5 samples returns Stable |
| T1-2 | `compute_trend` rising/falling/stable | Verify ±3% threshold behavior with known values |
| T1-3 | `compute_trend` zero average guard | All-zero buffer returns Stable, not divide-by-zero |
| T1-4 | `compute_trend_smooth` 20-sample window | Requires 20 samples, uses ±5% threshold |
| T1-5 | `compute_sigma` returns None below 10 | 9 samples → None, 10 → Some |
| T1-6 | `compute_sigma` correct value | Known dataset with verified σ |
| T1-7 | `compute_sigma` window parameter | Only uses last N samples, not entire buffer |
| T1-8 | RingBuffer capacity-1 edge case | Push 2 items, only last survives |
| T1-9 | RingBuffer clear | `clear()` resets len to 0 |
| T1-10 | TimeSeriesStore backfill clears old data | Pre-populate, backfill, verify old data gone |

### T2 — Unit Tests: Color Thresholds (`theme/colors.rs`)

All 4 tests implemented. Tests live in `theme/colors.rs` `#[cfg(test)]` module.

| ID | Test | Status | Description |
|----|------|--------|-------------|
| T2-1 | `temp_color` boundary tests | ✅ Done | < 70 → MINERAL, 70–79 → SANDSTONE, 80–89 → EMBER, 90–94 → MAGMA, ≥ 95 → ERUPTION |
| T2-2 | `util_color` boundary tests | ✅ Done | < 50 → LAVA, 50–84 → EMBER, ≥ 85 → MAGMA |
| T2-3 | `power_color` boundary tests | ✅ Done | < 40 → COPPER, 40–99 → EMBER, ≥ 100 → MAGMA |
| T2-4 | `with_alpha` preserves RGB | ✅ Done | Only alpha changes |

### T3 — Unit Tests: Chart Logic (`view/charts/line_chart.rs`)

| ID | Test | Description |
|----|------|-------------|
| T3-1 | `auto_range` single fixed bound | Only y_min or only y_max set, other auto-computed |
| T3-2 | `auto_range` threshold pulls up hi | Threshold close to data max gets included in range |
| T3-3 | `auto_range` NaN/Inf samples | Returns fallback (0.0, 100.0) |
| T3-4 | `auto_range` empty samples | Returns fallback |
| T3-5 | `nice_step` boundary values | Verify 1/2/5/10 snapping at magnitude boundaries |

### T4 — Unit Tests: App Logic (`app.rs`)

| ID | Test | Description |
|----|------|-------------|
| T4-1 | Node add deduplication | Same addr added twice → only one entry |
| T4-2 | Node remove clears state | Removes from nodes, node_order, and navigates to dashboard if viewing removed node |
| T4-3 | Dialog submit parsing | Full addr, bare IP, invalid input |
| T4-4 | Keyboard: Escape precedence | Dialog > overlay > detail view > no-op |
| T4-5 | Keyboard: number keys only in detail | 1/2/3 switch tabs only in detail view, no-op on dashboard |
| T4-6 | Keyboard: disabled during dialog | Number/arrow keys don't fire when dialog is open |
| T4-7 | Config save/load roundtrip | Write addresses, read them back, verify match |
| T4-8 | Workload overlay navigation | Prev/next clamp at bounds, open sets to last index |

### T5 — Unit Tests: Misc

| ID | Test | Description |
|----|------|-------------|
| T5-1 | `core_grid` busiest core selection | All cores ≤ 20% → None; one core at 50% → that core selected |
| T5-2 | `core_grid` color interpolation | `util_to_color(0.0)` = MINERAL, `util_to_color(1.0)` = ERUPTION, midpoint = EMBER |
| T5-3 | Connection backoff formula | Verify 1→2→4→8→16→30→30 progression, capped at MAX_BACKOFF_SECS |
| T5-4 | SSE event type routing | "snapshot"/"throttle"/"workload_start"/"workload_end" dispatch correctly, unknown types dropped, bad JSON → None |

### T6 — Integration Tests

| ID | Test | Size | Description |
|----|------|------|-------------|
| T6-1 | Mock server smoke test | M | Start a minimal HTTP server that serves `/system`, `/history`, and `/events` (with a few snapshot SSE messages). Connect tephra-client, verify data appears in NodeState after a few seconds. |
| T6-2 | Reconnection integration | M | Start mock server, connect, kill server, wait for reconnect backoff, restart server, verify client reconnects and resumes streaming. |
| T6-3 | Multi-node subscription | M | Start 2 mock servers on different ports, add both to client, verify independent SSE streams and node states. |
| T6-4 | Config persistence integration | S | Add a node, verify `~/.config/tephra/nodes.toml` exists with correct content. Remove node, verify config updated. Restart app, verify saved nodes auto-connect. |

---

## Open Questions

1. **Server-side changes** — Several features (mDNS registration, peak reset endpoint, TLS, longer history) require server changes. What's the server's development status and release cadence?
2. **Target node count** — Is the expected use case 2–5 nodes (homelab) or 20+ (small datacenter)? This affects whether P3-4 and P6-2 are worth prioritizing.
3. **Local history scope** — Should P2-2 store all nodes' history forever, or should there be a retention policy (e.g., 7 days, 100MB cap)?
4. **Packaging** — No packaging or distribution strategy yet. Should there be an AUR package, AppImage, or Flatpak?
