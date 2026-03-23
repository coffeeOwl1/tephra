# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Layout

Cargo workspace with two crates:

```
crates/server/   — tephra-server (monitoring agent, binary: tephra)
crates/client/   — tephra-client (iced GUI dashboard, binary: tephra-client)
```

## Build & Run

```bash
cargo build                                  # build everything
cargo build -p tephra-server                 # server only
cargo build -p tephra-client                 # client only
cargo build --release                        # release (opt-level 3, thin LTO, stripped)
cargo test --workspace                       # all tests
cargo test -p tephra-client -- test_name     # single test
cargo check --workspace                      # type check only
RUST_LOG=tephra_client=debug cargo run -p tephra-client  # verbose client logging
```

**Always build AND run the client after code changes.** This is a GUI app with wgpu rendering — compile-time checks alone aren't sufficient. Use `timeout 10 cargo run -p tephra-client 2>&1` for headless smoke tests. `RUST_BACKTRACE=1` is set automatically in the client's `main.rs`.

## Client Architecture

Elm-pattern GUI via iced 0.14 (wgpu backend, GPU-accelerated Canvas):

```
App::new()          → (State, Task<Message>)     — init + load config
App::update()       → Task<Message>              — pure state mutation
App::view()         → Element<Message>           — pure render
App::subscription() → Subscription<Message>      — SSE streams + keyboard
```

**Message flow:** `Message::Node(NodeId, NodeMessage)` wraps per-node SSE events. The subscription creates one `NodeRecipe` per connected node (identity keyed on `SocketAddr`). The recipe handles the full lifecycle: fetch `/api/v1/system`, backfill `/api/v1/history`, then stream `/api/v1/events` via SSE with automatic reconnection (exponential backoff 1s→30s).

**Views:** `App.current_view` is either `Dashboard`, `Detail { node_id, tab }`, or `Compare`. Modal overlays (add-node dialog, workload overlay) stack via `iced::widget::stack!`.

## Key Client Modules

- **`app.rs`** — App state, all message handling, keyboard shortcuts, config save/load, view routing
- **`node/mod.rs`** — `NodeState` with all per-node tracking: snapshots, history, throttle time, temp duration arrays, efficiency baseline, client-side workload detection
- **`node/connection.rs`** — `NodeRecipe` implementing `iced::advanced::subscription::Recipe` for SSE lifecycle
- **`node/history.rs`** — `RingBuffer<T>` (240 samples / 2 min), `TimeSeriesStore`, trend computation (`compute_trend` / `compute_trend_smooth`), sigma
- **`view/charts/line_chart.rs`** — Canvas-based chart with auto-ranging Y axis, area fill, grid lines, peak line, glow dot. Uses persistent `canvas::Cache` cleared on each snapshot
- **`view/charts/core_grid.rs`** — Per-core heatmap built from iced widgets (NOT Canvas)
- **`view/compare.rs`** — Multi-node comparison dashboard with overlay charts, sortable summary, event console
- **`theme/colors.rs`** — Volcanic color palette with geological names (OBSIDIAN, BASALT, EMBER, MAGMA, etc.) and threshold functions (`temp_color`, `util_color`, `power_color`)

## Key Server Modules

- **`main.rs`** — CLI parsing (clap), tokio runtime, server startup, signal handling
- **`api.rs`** — Axum HTTP routes, SSE streaming, sampling loop
- **`monitor.rs`** — Core sampling logic, sensor reading, throttle detection, workload tracking
- **`models.rs`** — Serde-serializable API response types
- **`discovery.rs`** — mDNS service registration (`_tephra._tcp.local.`)

## Conventions

**Color naming:** All colors use geological names from the volcanic theme. Never use generic names like "red" or "warning_color" — use `MAGMA`, `ERUPTION`, `EMBER`, etc.

**Trend computation:** Two modes — `compute_trend()` (5-sample / 2.5s window, ±3% threshold) for stable signals like temperature, and `compute_trend_smooth()` (20-sample / 10s window, ±5%) for noisy signals like power.

**Temperature duration:** `temp_duration_ticks[t]` counts at-exactly degree `t`. `temp_streak_current/max[t]` track streaks at-or-above degree `t`. Display uses `cumulative_temp_secs()` which sums `temp_duration_ticks[t..=105]`.

**Canvas vs widgets:** Line charts and sparklines use `canvas::Program` with persistent `Cache`. The core grid uses regular iced widgets because it doesn't need custom drawing. Never mix Canvas text (`fill_text`) with iced `text()` widgets inside the same component.

**Subscriptions:** iced 0.14's `Subscription::run` and `run_with` require `fn` pointers (not closures). Use `iced::advanced::subscription::from_recipe` with a custom `Recipe` impl when you need to capture state.

## API

The tephra-server runs on port 9867 and exposes:
- `GET /api/v1/system` — static node info (hostname, CPU model, core count, etc.)
- `GET /api/v1/snapshot` — current metrics point-in-time
- `GET /api/v1/history` — last 120 samples as parallel arrays
- `GET /api/v1/events` — SSE stream: `snapshot` (every 500ms), `throttle`, `workload_start`, `workload_end`
- `GET /health` — health check

Dev server: `192.168.99.80:9867`. Config persists to `~/.config/tephra/nodes.toml`.

## Rendering Notes

- **iced 0.14 + wgpu is required.** iced 0.13's tiny-skia fallback renderer has broken Canvas positioning and an `f32` sort panic on Rust 1.81+. Do not downgrade.
- Canvas `cache.draw()` uses `bounds.size()` as cache key. Caches are cleared via `ChartCaches::clear_all()` on every snapshot (~500ms).
- The `auto_range()` function in `line_chart.rs` supports optional fixed Y bounds (`y_min`/`y_max` on `LineChartConfig`). Temperature uses fixed 25–100°C.

## Existing Tests

Client tests live as `#[cfg(test)]` modules in their respective source files:
- `net/api_types.rs` — 6 tests: real payload parsing for all API types
- `node/history.rs` — 6 tests: RingBuffer operations, TimeSeriesStore backfill
- `view/charts/line_chart.rs` — 4 tests: auto_range, nice_step, fixed bounds

Server integration tests live in `crates/server/tests/api.rs`:
- 12 tests: health, system info, snapshot, history, OpenAPI, SSE, 404, value sanity
