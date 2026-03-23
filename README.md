# Tephra

GPU-accelerated multi-node CPU thermal monitoring for Linux. Tephra consists of a lightweight server agent that reads hardware sensors on each machine, and a desktop client that visualizes real-time telemetry across your fleet.

![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-green)

## Overview

**tephra-server** is a monitoring agent that runs on each Linux machine. It reads CPU temperature, power, frequency, utilization, and fan speed from standard kernel interfaces and exposes them over HTTP with Server-Sent Events for live streaming.

**tephra-client** is an iced/wgpu desktop application that connects to one or more tephra servers and displays real-time dashboards with charts, heatmaps, throttle detection, workload tracking, and multi-node comparison.

## Quick Start

```bash
git clone https://github.com/coffeeOwl1/tephra.git
cd tephra

# Interactive installer — handles dependencies, lets you pick server/client/both
./install.sh
```

Or build manually:

```bash
# Server (monitoring agent)
cargo build --release -p tephra-server
sudo cp target/release/tephra /usr/local/bin/

# Client (GUI dashboard)
cargo build --release -p tephra-client
./target/release/tephra-client
```

## Install Script

The interactive `install.sh` handles everything:

1. **Detects your distro** (Arch, Ubuntu/Debian, Fedora, openSUSE)
2. **Installs Rust** via rustup if not present
3. **Installs system dependencies** for the client (Vulkan drivers, fontconfig, etc.)
4. **Prompts you to choose**: server only, client only, or both
5. **Builds and installs** binaries to `/usr/local/bin/`
6. **Optionally sets up a systemd service** for the server

## Server

### What It Monitors

| Metric | Source |
|--------|--------|
| Temperature | k10temp (AMD), coretemp (Intel), zenpower, thermal_zone fallback |
| Power | Intel RAPL (works on AMD too) |
| Frequency | Per-core from cpufreq |
| Utilization | Per-core from `/proc/stat` |
| Fan speed | hwmon fan sensors (optional) |
| Throttling | Heuristic: thermal (>85°C) vs power based on core freq ratios |
| Workloads | Auto-detected sustained CPU activity with per-workload stats |

Supports **x86_64** and **aarch64** (ARM64).

### Running the Server

```bash
# Default: port 9867, 500ms sampling
tephra

# Custom settings
tephra --port 8080 --interval 1000
```

### Systemd Service

The install script can set this up automatically, or manually:

```bash
sudo cp tephra.service /etc/systemd/system/
sudo systemctl enable --now tephra
```

```bash
systemctl status tephra          # check status
journalctl -u tephra -f          # follow logs
curl http://localhost:9867/health # verify
```

### Docker

```bash
docker build -t tephra .
docker run -d --privileged -v /sys:/sys:ro -v /proc:/proc:ro -p 9867:9867 tephra
```

### API

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check (`{"status":"ok"}`) |
| `GET /api/v1/system` | Static info: hostname, CPU, cores, RAM, governor |
| `GET /api/v1/snapshot` | Current metrics with per-core breakdown |
| `GET /api/v1/history` | Last 120 samples (60s at 500ms) |
| `GET /api/v1/events` | SSE stream: snapshots (500ms), throttle, workload events |
| `GET /api/v1/openapi.json` | OpenAPI 3.1 spec |

### mDNS Discovery

The server registers as `_tephra._tcp.local.` via mDNS. The client can auto-discover agents on your LAN without manual IP entry.

### Multi-Machine Deployment

For each machine you want to monitor:

```bash
git clone https://github.com/coffeeOwl1/tephra.git
cd tephra
./install.sh   # choose "1) Server only"
```

The install script handles building and systemd setup. For headless machines, SSH in and run the same commands.

## Client

### Features

- **Real-time dashboard** with card grid, live sparklines, fan/throttle status
- **Detail view** with three tabs: Overview (line charts), Cores (per-core heatmap), Events (throttle log + workloads)
- **Comparison dashboard** with overlay charts, fleet power tracking, sortable summary table, filterable event console
- **Auto-discovery** scans your /24 subnet for servers
- **GPU-rendered charts** via iced 0.14 + wgpu
- **Workload detection** with per-workload thermal statistics
- **Config persistence** at `~/.config/tephra/nodes.toml`

### System Requirements

- Linux with Vulkan-capable GPU (for wgpu rendering)
- At least one tephra server running on your network

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| **Esc** | Back to dashboard |
| **a** | Add node dialog |
| **d** | Toggle comparison view |
| **p** | Pause/resume streaming |
| **c** | Toggle compact mode |
| **r** | Retry failed connection |
| **w** | Open workload overlay |
| **1/2/3** | Switch detail tabs |
| **Left/Right** | Navigate workloads / switch nodes |

## Project Structure

```
tephra/
├── crates/
│   ├── server/    # monitoring agent (tephra binary)
│   └── client/    # GUI dashboard (tephra-client binary)
├── install.sh     # unified installer
├── Dockerfile     # server container build
└── tephra.service # systemd unit
```

This is a Cargo workspace. Build individual crates with:

```bash
cargo build -p tephra-server
cargo build -p tephra-client
cargo test --workspace
```

## License

MIT
