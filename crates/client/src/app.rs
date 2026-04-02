use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use iced::keyboard;
use iced::widget::{container, Space};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::message::{DetailTab, EventFilter, Message, NodeMessage, SummaryColumn};
use crate::node::connection::node_subscription;
use crate::node::{NodeId, NodeState};
use crate::theme;
use crate::view;

/// Canvas caches for comparison view (shared across all nodes).
pub struct CompareCaches {
    pub temp: iced::widget::canvas::Cache,
    pub power: iced::widget::canvas::Cache,
    pub freq: iced::widget::canvas::Cache,
    pub util: iced::widget::canvas::Cache,
    pub fleet_power: iced::widget::canvas::Cache,
}

impl CompareCaches {
    pub fn new() -> Self {
        Self {
            temp: iced::widget::canvas::Cache::new(),
            power: iced::widget::canvas::Cache::new(),
            freq: iced::widget::canvas::Cache::new(),
            util: iced::widget::canvas::Cache::new(),
            fleet_power: iced::widget::canvas::Cache::new(),
        }
    }

    pub fn clear_all(&self) {
        self.temp.clear();
        self.power.clear();
        self.freq.clear();
        self.util.clear();
        self.fleet_power.clear();
    }
}

/// Which view is currently displayed.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Dashboard,
    Detail { node_id: NodeId, tab: DetailTab },
    Compare,
    Settings,
}

/// Main application state.
pub struct App {
    pub nodes: BTreeMap<NodeId, NodeState>,
    pub node_order: Vec<NodeId>,
    pub current_view: View,
    pub show_add_dialog: bool,
    pub add_dialog_input: String,
    pub add_dialog_error: Option<String>,
    pub http_client: reqwest::Client,
    /// Workload overlay: index into the current detail node's completed_workloads.
    pub workload_overlay_idx: Option<usize>,
    /// Whether display updates are paused.
    pub paused: bool,
    /// Compact detail view (hides temp duration, fan chart).
    pub compact: bool,
    /// Pending display names from config (applied when nodes are added).
    pending_display_names: std::collections::HashMap<String, String>,
    /// Canvas caches for comparison view charts.
    pub compare_caches: CompareCaches,
    /// Last time compare caches were cleared (throttle redraws to avoid jitter).
    compare_last_redraw: std::time::Instant,
    /// Fleet power history (sum of all nodes' ppt_watts per tick).
    pub fleet_power_history: crate::node::history::RingBuffer<f64>,
    /// Chart history duration preset.
    pub history_duration: crate::node::history::HistoryDuration,
    /// Event console filter for Compare view.
    pub console_filter: EventFilter,
    /// Summary table sort column and ascending flag.
    pub summary_sort: (SummaryColumn, bool),
    /// Global alert defaults.
    pub alert_defaults: crate::alerts::AlertDefaults,
    /// Pending alert overrides from config (applied when nodes are added).
    pending_alert_overrides: std::collections::HashMap<String, crate::alerts::AlertOverrides>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        let mut app = Self {
            nodes: BTreeMap::new(),
            node_order: Vec::new(),
            current_view: View::Dashboard,
            show_add_dialog: false,
            add_dialog_input: String::new(),
            add_dialog_error: None,
            http_client: client,
            workload_overlay_idx: None,
            paused: false,
            compact: false,
            pending_display_names: std::collections::HashMap::new(),
            compare_caches: CompareCaches::new(),
            compare_last_redraw: std::time::Instant::now(),
            fleet_power_history: crate::node::history::RingBuffer::new(
                crate::node::history::HistoryDuration::default().samples(),
            ),
            history_duration: crate::node::history::HistoryDuration::default(),
            console_filter: EventFilter::default(),
            summary_sort: (SummaryColumn::default(), false), // Temp descending
            alert_defaults: crate::alerts::AlertDefaults::default(),
            pending_alert_overrides: std::collections::HashMap::new(),
        };

        // Load saved nodes from config
        let loaded = load_config();
        if let Some(dur) = loaded.history_duration {
            app.history_duration = dur;
            app.fleet_power_history.resize(dur.samples());
        }
        app.alert_defaults = loaded.alert_defaults;
        let saved = loaded.nodes;
        for sn in &saved {
            if let Some(name) = &sn.display_name {
                app.pending_display_names
                    .insert(sn.addr.to_string(), name.clone());
            }
            if let Some(overrides) = &sn.alert_overrides {
                app.pending_alert_overrides
                    .insert(sn.addr.to_string(), overrides.clone());
            }
        }
        // Collect saved node addresses for localhost check
        let saved_addrs: Vec<SocketAddr> = saved.iter().map(|sn| sn.addr).collect();

        let mut tasks: Vec<Task<Message>> = saved
            .into_iter()
            .map(|sn| Task::done(Message::AddNode(sn.addr)))
            .collect();

        // Auto-discover tephra servers on the local subnet
        let client = app.http_client.clone();
        let existing = saved_addrs.clone();
        tasks.push(Task::perform(
            discover_subnet(client, existing),
            Message::DiscoveredNodes,
        ));

        let task = if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        };

        (app, task)
    }

    pub fn title(&self) -> String {
        match &self.current_view {
            View::Dashboard => "Tephra".to_string(),
            View::Detail { node_id, .. } => {
                if let Some(node) = self.nodes.get(node_id) {
                    format!("Tephra — {}", node.display_name())
                } else {
                    "Tephra".to_string()
                }
            }
            View::Compare => "Tephra \u{2014} Compare".to_string(),
            View::Settings => "Tephra \u{2014} Settings".to_string(),
        }
    }

    pub fn theme(&self) -> Theme {
        theme::tephra_theme()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddNode(addr) => {
                if self.nodes.values().any(|n| n.addr == addr) {
                    return Task::none();
                }
                let id = NodeId::new();
                let mut node = NodeState::with_capacity(id, addr, self.history_duration.samples());
                // Apply pending display name and alert overrides from config
                let addr_str = addr.to_string();
                if let Some(name) = self.pending_display_names.remove(&addr_str) {
                    node.custom_name = Some(name);
                }
                if let Some(overrides) = self.pending_alert_overrides.remove(&addr_str) {
                    node.alert_overrides = overrides;
                }
                self.nodes.insert(id, node);
                self.node_order.push(id);
                self.show_add_dialog = false;
                self.add_dialog_input.clear();
                self.save_config_full();
                Task::none()
            }

            Message::DiscoveredNodes(addrs) => {
                let new: Vec<_> = addrs
                    .into_iter()
                    .filter(|addr| !self.nodes.values().any(|n| n.addr == *addr))
                    .collect();
                if new.is_empty() {
                    return Task::none();
                }
                tracing::info!("Auto-discovered {} tephra server(s)", new.len());
                return Task::batch(new.into_iter().map(|a| Task::done(Message::AddNode(a))));
            }

            Message::RemoveNode(id) => {
                self.nodes.remove(&id);
                self.node_order.retain(|nid| *nid != id);
                self.save_config_full();
                if let View::Detail { node_id, .. } = &self.current_view {
                    if *node_id == id {
                        self.current_view = View::Dashboard;
                    }
                }
                Task::none()
            }

            Message::RetryConnection(id) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.status = crate::node::ConnectionStatus::Connecting;
                }
                Task::none()
            }

            Message::SetDisplayName(id, name) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.custom_name = name;
                    self.save_config_full();
                }
                Task::none()
            }

            Message::Node(id, node_msg) => {
                // Hostname dedup on SystemInfoFetched: check before
                // taking a mutable borrow
                if let NodeMessage::SystemInfoFetched(ref info) = node_msg {
                    let dominated = self.nodes.iter().any(|(&other_id, other)| {
                        other_id != id
                            && other
                                .system_info
                                .as_ref()
                                .is_some_and(|si| si.hostname == info.hostname)
                    });
                    if dominated {
                        if let Some(n) = self.nodes.get(&id) {
                            tracing::info!(
                                "Removing duplicate node {} (hostname {} already connected)",
                                n.addr,
                                info.hostname
                            );
                        }
                        self.nodes.remove(&id);
                        self.node_order.retain(|nid| *nid != id);
                        self.save_config_full();
                        return Task::none();
                    }
                }

                if let Some(node) = self.nodes.get_mut(&id) {
                    match node_msg {
                        NodeMessage::SystemInfoFetched(info) => {
                            // Check server version compatibility
                            const MIN_SERVER_VERSION: &str = "0.1.0";
                            if !info.agent_version.is_empty()
                                && info.agent_version < MIN_SERVER_VERSION.to_string()
                            {
                                node.version_warning = Some(format!(
                                    "Server v{} may be incompatible (minimum: v{})",
                                    info.agent_version, MIN_SERVER_VERSION
                                ));
                                tracing::warn!(
                                    "Node {}: server version {} below minimum {}",
                                    node.display_name(),
                                    info.agent_version,
                                    MIN_SERVER_VERSION
                                );
                            }
                            node.system_info = Some(info);
                            node.status = crate::node::ConnectionStatus::FetchingInfo;
                        }
                        NodeMessage::HistoryFetched(hist) => {
                            node.history.backfill(&hist);
                            node.caches.clear_all();
                        }
                        NodeMessage::Connected => {
                            node.status = crate::node::ConnectionStatus::Streaming;
                        }
                        NodeMessage::SnapshotReceived(snap) => {
                            if !self.paused {
                                node.on_snapshot(snap);
                                // Selective cache invalidation based on current view
                                let is_viewed_detail = matches!(
                                    &self.current_view,
                                    View::Detail { node_id: nid, .. } if *nid == id
                                );
                                // Sparkline always needs clearing (visible on dashboard)
                                node.caches.sparkline.clear();
                                if is_viewed_detail {
                                    node.caches.temp.clear();
                                    node.caches.power.clear();
                                    node.caches.freq.clear();
                                    node.caches.util.clear();
                                    node.caches.fan.clear();
                                    node.caches.core_grid.clear();
                                }
                                if matches!(self.current_view, View::Compare)
                                    && self.compare_last_redraw.elapsed()
                                        >= std::time::Duration::from_millis(450)
                                {
                                    // Compute fleet power sum
                                    let fleet_sum: f64 = self
                                        .nodes
                                        .values()
                                        .filter_map(|n| n.snapshot.as_ref())
                                        .map(|s| s.ppt_watts)
                                        .sum();
                                    self.fleet_power_history.push(fleet_sum);
                                    self.compare_caches.clear_all();
                                    self.compare_last_redraw = std::time::Instant::now();
                                }
                            }
                        }
                        NodeMessage::ThrottleEvent(evt) => {
                            node.on_throttle(evt);
                        }
                        NodeMessage::WorkloadStart(evt) => {
                            node.on_workload_start(evt);
                        }
                        NodeMessage::WorkloadEnd(evt) => {
                            node.on_workload_end(evt);
                        }
                        NodeMessage::Disconnected(reason) => {
                            node.status = crate::node::ConnectionStatus::Reconnecting(0);
                            tracing::warn!(
                                "Node {} disconnected: {}",
                                node.display_name(),
                                reason
                            );
                        }
                        NodeMessage::ConnectionFailed(reason) => {
                            tracing::error!(
                                "Node {} connection failed: {}",
                                node.display_name(),
                                reason
                            );
                            node.status =
                                crate::node::ConnectionStatus::Failed(reason);
                        }
                    }

                }
                // Evaluate and dispatch alerts (re-acquire borrow to avoid
                // conflict with fleet_sum's immutable borrow of self.nodes)
                if let Some(node) = self.nodes.get_mut(&id) {
                    let alerts =
                        crate::alerts::check_alerts(&self.alert_defaults, node);
                    let timeout = self.alert_defaults.notification_timeout_secs;
                    for alert in alerts {
                        crate::alerts::send_notification(alert, timeout);
                    }
                }
                Task::none()
            }

            Message::NavigateDashboard => {
                self.current_view = View::Dashboard;
                Task::none()
            }

            Message::NavigateCompare => {
                self.current_view = View::Compare;
                // Clear all caches so charts redraw with current data
                for node in self.nodes.values() {
                    node.caches.clear_all();
                }
                Task::none()
            }

            Message::NavigateDetail(id) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.mark_events_viewed();
                    node.caches.clear_all(); // Redraw all charts when entering detail
                }
                // Preserve current tab when switching nodes within detail view
                let tab = match &self.current_view {
                    View::Detail { tab, .. } => *tab,
                    _ => DetailTab::Overview,
                };
                self.current_view = View::Detail { node_id: id, tab };
                Task::none()
            }

            Message::SwitchTab(new_tab) => {
                if let View::Detail { ref mut tab, .. } = self.current_view {
                    *tab = new_tab;
                }
                Task::none()
            }

            Message::OpenAddDialog => {
                self.show_add_dialog = true;
                self.add_dialog_error = None;
                Task::none()
            }

            Message::CloseAddDialog => {
                self.show_add_dialog = false;
                self.add_dialog_input.clear();
                self.add_dialog_error = None;
                Task::none()
            }

            Message::AddDialogInput(value) => {
                self.add_dialog_input = value;
                self.add_dialog_error = None;
                Task::none()
            }

            Message::AddDialogSubmit => {
                let input = self.add_dialog_input.trim();
                if input.is_empty() {
                    self.add_dialog_error = Some("Enter an IP address".into());
                    return Task::none();
                }
                let addr = if let Ok(a) = input.parse::<SocketAddr>() {
                    a
                } else if let Ok(ip) = input.parse::<std::net::IpAddr>() {
                    SocketAddr::new(ip, 9867)
                } else {
                    self.add_dialog_error =
                        Some("Invalid format — use IP or IP:port".into());
                    return Task::none();
                };
                // Duplicate check
                if self.nodes.values().any(|n| n.addr == addr) {
                    self.add_dialog_error = Some("Node already added".into());
                    return Task::none();
                }
                self.add_dialog_error = None;
                Task::done(Message::AddNode(addr))
            }

            Message::KeyPressed(key) => {
                use keyboard::Key::{Named, Character};
                use keyboard::key::Named as N;

                // Don't handle shortcuts when add dialog is open (except Escape)
                let in_dialog = self.show_add_dialog;

                match key {
                    Named(N::Escape) => {
                        if in_dialog {
                            self.show_add_dialog = false;
                            self.add_dialog_input.clear();
                            self.add_dialog_error = None;
                        } else if self.workload_overlay_idx.is_some() {
                            self.workload_overlay_idx = None;
                        } else if matches!(
                            self.current_view,
                            View::Detail { .. } | View::Compare | View::Settings
                        ) {
                            self.current_view = View::Dashboard;
                        }
                    }

                    // Tab switching: 1=Overview, 2=Cores, 3=Events
                    Character(ref c) if !in_dialog => match c.as_str() {
                        "1" => {
                            if let View::Detail { ref mut tab, .. } = self.current_view {
                                *tab = DetailTab::Overview;
                            }
                        }
                        "2" => {
                            if let View::Detail { ref mut tab, .. } = self.current_view {
                                *tab = DetailTab::Cores;
                            }
                        }
                        "3" => {
                            if let View::Detail { ref mut tab, .. } = self.current_view {
                                *tab = DetailTab::Events;
                            }
                        }
                        "4" => {
                            if let View::Detail { ref mut tab, .. } = self.current_view {
                                *tab = DetailTab::History;
                            }
                        }
                        "5" => {
                            if let View::Detail { ref mut tab, .. } = self.current_view {
                                *tab = DetailTab::Alerts;
                            }
                        }
                        // 'a' — open add node dialog
                        "a" => {
                            self.show_add_dialog = true;
                        }
                        // 'w' — toggle workload overlay
                        "w" => {
                            if self.workload_overlay_idx.is_some() {
                                self.workload_overlay_idx = None;
                            } else {
                                return Task::done(Message::OpenWorkloadOverlay);
                            }
                        }
                        // 'r' — reset client-side peaks and stats
                        "r" => {
                            if let View::Detail { node_id, .. } = &self.current_view {
                                let nid = *node_id;
                                if let Some(node) = self.nodes.get_mut(&nid) {
                                    node.reset_client_state();
                                }
                            }
                        }
                        // 'p' — toggle pause
                        "p" => {
                            self.paused = !self.paused;
                        }
                        // 'c' — toggle compact mode
                        "c" => {
                            self.compact = !self.compact;
                        }
                        // 'd' — compare dashboard
                        "d" => {
                            if matches!(self.current_view, View::Compare) {
                                self.current_view = View::Dashboard;
                            } else {
                                return Task::done(Message::NavigateCompare);
                            }
                        }
                        _ => {}
                    },

                    // Left/Right: workload overlay nav when open, else node nav
                    Named(N::ArrowLeft) if !in_dialog => {
                        if let Some(idx) = &mut self.workload_overlay_idx {
                            if *idx > 0 {
                                *idx -= 1;
                            }
                        } else if let View::Detail { ref mut node_id, .. } = self.current_view {
                            if let Some(pos) = self.node_order.iter().position(|id| id == node_id) {
                                if pos > 0 {
                                    *node_id = self.node_order[pos - 1];
                                }
                            }
                        }
                    }
                    Named(N::ArrowRight) if !in_dialog => {
                        if let Some(idx) = &mut self.workload_overlay_idx {
                            if let View::Detail { node_id, .. } = &self.current_view {
                                if let Some(node) = self.nodes.get(node_id) {
                                    if *idx + 1 < node.completed_workloads.len() {
                                        *idx += 1;
                                    }
                                }
                            }
                        } else if let View::Detail { ref mut node_id, .. } = self.current_view {
                            if let Some(pos) = self.node_order.iter().position(|id| id == node_id) {
                                if pos + 1 < self.node_order.len() {
                                    *node_id = self.node_order[pos + 1];
                                }
                            }
                        }
                    }

                    _ => {}
                }
                Task::none()
            }

            Message::OpenWorkloadOverlay => {
                if let View::Detail { node_id, .. } = &self.current_view {
                    if let Some(node) = self.nodes.get(node_id) {
                        if !node.completed_workloads.is_empty() {
                            self.workload_overlay_idx = Some(node.completed_workloads.len() - 1);
                        }
                    }
                }
                Task::none()
            }

            Message::CloseWorkloadOverlay => {
                self.workload_overlay_idx = None;
                Task::none()
            }

            Message::WorkloadOverlayPrev => {
                if let Some(idx) = &mut self.workload_overlay_idx {
                    if *idx > 0 {
                        *idx -= 1;
                    }
                }
                Task::none()
            }

            Message::WorkloadOverlayNext => {
                if let Some(idx) = &mut self.workload_overlay_idx {
                    if let View::Detail { node_id, .. } = &self.current_view {
                        if let Some(node) = self.nodes.get(node_id) {
                            if *idx + 1 < node.completed_workloads.len() {
                                *idx += 1;
                            }
                        }
                    }
                }
                Task::none()
            }

            Message::SetConsoleFilter(filter) => {
                self.console_filter = filter;
                Task::none()
            }

            Message::SetSummarySort(col) => {
                if self.summary_sort.0 == col {
                    self.summary_sort.1 = !self.summary_sort.1; // toggle direction
                } else {
                    self.summary_sort = (col, false); // new column, descending
                }
                Task::none()
            }

            Message::NavigateSettings => {
                self.current_view = View::Settings;
                Task::none()
            }

            Message::SetHistoryDuration(duration) => {
                self.history_duration = duration;
                let cap = duration.samples();
                for node in self.nodes.values_mut() {
                    node.history.resize(cap);
                }
                self.fleet_power_history.resize(cap);
                self.save_config_full();
                Task::none()
            }

            Message::UpdateAlertDefaults(defaults) => {
                self.alert_defaults = defaults;
                self.save_config_full();
                Task::none()
            }

            Message::SetNodeAlertOverride(id, overrides) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.alert_overrides = overrides;
                    self.save_config_full();
                }
                Task::none()
            }

            Message::ClearNodeAlertOverrides(id) => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.alert_overrides = Default::default();
                    self.save_config_full();
                }
                Task::none()
            }

            Message::SendTestNotification => {
                crate::alerts::send_notification(
                    crate::alerts::AlertNotification {
                        notification: crate::alerts::PendingNotification::TempCeiling {
                            node_name: "Test".to_string(),
                            temp: 85,
                            ceiling: 85,
                            timestamp: chrono::Local::now(),
                        },
                        persistent: false,
                    },
                    self.alert_defaults.notification_timeout_secs,
                );
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = match &self.current_view {
            View::Dashboard => view::dashboard::view(self),
            View::Detail { node_id, tab } => {
                if let Some(node) = self.nodes.get(node_id) {
                    view::detail::view(self, node, *tab)
                } else {
                    view::dashboard::view(self)
                }
            }
            View::Compare => view::compare::view(self),
            View::Settings => view::settings::view(self),
        };

        // Check for overlays
        let has_overlay = self.show_add_dialog || self.workload_overlay_idx.is_some();

        if has_overlay {
            let base = container(content)
                .width(Length::Fill)
                .height(Length::Fill);

            let overlay: Element<'_, Message> = if self.show_add_dialog {
                view::components::add_node_dialog::add_node_dialog(
                    &self.add_dialog_input,
                    self.add_dialog_error.as_deref(),
                )
            } else if let (Some(idx), View::Detail { node_id, .. }) =
                (self.workload_overlay_idx, &self.current_view)
            {
                if let Some(node) = self.nodes.get(node_id) {
                    view::workload_overlay::workload_overlay(node, idx)
                } else {
                    Space::new().into()
                }
            } else {
                Space::new().into()
            };

            iced::widget::stack![base, overlay].into()
        } else {
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let node_subs: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                !matches!(n.status, crate::node::ConnectionStatus::Failed(_))
            })
            .map(|(_, n)| node_subscription(n.id, n.addr, self.http_client.clone()))
            .collect();

        // Keyboard events
        let keyboard_sub = iced::event::listen_with(|event, _status, _id| {
            if let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
                Some(Message::KeyPressed(key))
            } else {
                None
            }
        });

        Subscription::batch(node_subs.into_iter().chain(std::iter::once(keyboard_sub)))
    }

    /// Save full config (nodes, alert defaults, per-node overrides) to disk.
    fn save_config_full(&self) {
        let nodes: Vec<SavedNodeV2> = self
            .node_order
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .map(|n| SavedNodeV2 {
                addr: n.addr.to_string(),
                display_name: n.custom_name.clone(),
                alerts: if n.alert_overrides.has_any() {
                    Some(n.alert_overrides.clone())
                } else {
                    None
                },
            })
            .collect();
        let cfg = SavedConfigV2 {
            version: CONFIG_VERSION,
            alerts: self.alert_defaults.clone(),
            history_duration: if self.history_duration == crate::node::history::HistoryDuration::default() {
                None
            } else {
                Some(self.history_duration)
            },
            nodes,
        };
        write_config_v2(&cfg);
    }
}

/// Config file path: ~/.config/tephra/nodes.toml
fn config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("tephra").join("nodes.toml"))
}

/// Current config version.
const CONFIG_VERSION: u32 = 2;

// -- Config V1 (legacy) ------------------------------------------------

#[allow(dead_code)]
#[derive(serde::Deserialize, Default)]
struct SavedConfigV1 {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    nodes: Vec<String>,
    #[serde(default)]
    display_names: std::collections::HashMap<String, String>,
    #[serde(default)]
    history_capacity: Option<usize>,
}

// -- Config V2 ----------------------------------------------------------

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct SavedNodeV2 {
    addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alerts: Option<crate::alerts::AlertOverrides>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct SavedConfigV2 {
    version: u32,
    #[serde(default)]
    alerts: crate::alerts::AlertDefaults,
    #[serde(default)]
    history_duration: Option<crate::node::history::HistoryDuration>,
    #[serde(default)]
    nodes: Vec<SavedNodeV2>,
}

// -- Internal types -----------------------------------------------------

struct SavedNode {
    addr: SocketAddr,
    display_name: Option<String>,
    alert_overrides: Option<crate::alerts::AlertOverrides>,
}

struct LoadedConfig {
    nodes: Vec<SavedNode>,
    alert_defaults: crate::alerts::AlertDefaults,
    history_duration: Option<crate::node::history::HistoryDuration>,
}

impl Default for LoadedConfig {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            alert_defaults: crate::alerts::AlertDefaults::default(),
            history_duration: None,
        }
    }
}

fn parse_addr(s: &str) -> Option<SocketAddr> {
    s.parse::<SocketAddr>().ok().or_else(|| {
        s.parse::<std::net::IpAddr>()
            .ok()
            .map(|ip| SocketAddr::new(ip, 9867))
    })
}

/// Load saved nodes from config (with v1 → v2 migration).
fn load_config() -> LoadedConfig {
    let path = match config_path() {
        Some(p) => p,
        None => return LoadedConfig::default(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return LoadedConfig::default(),
    };

    // Try v2 first
    if let Ok(cfg) = toml::from_str::<SavedConfigV2>(&content) {
        if cfg.version >= 2 {
            let nodes = cfg
                .nodes
                .iter()
                .filter_map(|n| {
                    let addr = parse_addr(&n.addr)?;
                    Some(SavedNode {
                        addr,
                        display_name: n.display_name.clone(),
                        alert_overrides: n.alerts.clone(),
                    })
                })
                .collect();
            return LoadedConfig {
                nodes,
                alert_defaults: cfg.alerts,
                history_duration: cfg.history_duration,
            };
        }
    }

    // Fall back to v1
    if let Ok(old) = toml::from_str::<SavedConfigV1>(&content) {
        tracing::info!("Migrating config from v{} to v{CONFIG_VERSION}", old.version);
        let nodes: Vec<SavedNode> = old
            .nodes
            .iter()
            .filter_map(|s| {
                let addr = parse_addr(s)?;
                let display_name = old.display_names.get(&addr.to_string()).cloned();
                Some(SavedNode {
                    addr,
                    display_name,
                    alert_overrides: None,
                })
            })
            .collect();
        let loaded = LoadedConfig {
            nodes,
            alert_defaults: crate::alerts::AlertDefaults::default(),
            history_duration: None,
        };
        // Auto-upgrade: write v2 config
        let v2 = SavedConfigV2 {
            version: CONFIG_VERSION,
            alerts: loaded.alert_defaults.clone(),
            history_duration: None,
            nodes: loaded
                .nodes
                .iter()
                .map(|n| SavedNodeV2 {
                    addr: n.addr.to_string(),
                    display_name: n.display_name.clone(),
                    alerts: None,
                })
                .collect(),
        };
        write_config_v2(&v2);
        return loaded;
    }

    tracing::warn!("Failed to parse config {}", path.display());
    LoadedConfig::default()
}

/// Write a v2 config to disk.
fn write_config_v2(cfg: &SavedConfigV2) {
    let path = match config_path() {
        Some(p) => p,
        None => return,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = match toml::to_string_pretty(cfg) {
        Ok(c) => format!("# Tephra client config\n{c}"),
        Err(e) => {
            tracing::warn!("Failed to serialize config: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, content) {
        tracing::warn!("Failed to save config {}: {e}", path.display());
    }
}

/// Discover tephra servers on the local /24 subnet + localhost.
async fn discover_subnet(
    client: reqwest::Client,
    existing: Vec<SocketAddr>,
) -> Vec<SocketAddr> {
    // Find local IP via UDP socket trick
    let local_ip = match find_local_ip() {
        Some(IpAddr::V4(v4)) => v4,
        _ => {
            tracing::debug!("Could not determine local IPv4 address for discovery");
            return Vec::new();
        }
    };

    let octets = local_ip.octets();
    tracing::info!(
        "Scanning subnet {}.{}.{}.0/24 for tephra servers",
        octets[0],
        octets[1],
        octets[2]
    );

    // Probe all 254 hosts in parallel (includes local machine at its LAN IP)
    let mut handles = Vec::with_capacity(254);
    for i in 1..=254u8 {
        let ip = Ipv4Addr::new(octets[0], octets[1], octets[2], i);
        let addr = SocketAddr::new(IpAddr::V4(ip), 9867);
        if existing.contains(&addr) {
            continue; // Skip already-configured nodes
        }
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            probe_single(&c, addr).await
        }));
    }

    let mut found = Vec::new();
    for handle in handles {
        if let Ok(Some(addr)) = handle.await {
            found.push(addr);
        }
    }

    if found.is_empty() {
        return found;
    }

    // Deduplicate by hostname — a machine with multiple IPs (WiFi + Ethernet)
    // would otherwise appear multiple times
    let mut seen_hostnames = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for addr in &found {
        let hostname = match client
            .get(format!("http://{addr}/api/v1/system"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("hostname")?.as_str().map(String::from)),
            Err(_) => None,
        };

        match hostname {
            Some(name) if !seen_hostnames.insert(name.clone()) => {
                tracing::debug!("Skipping duplicate {addr} (hostname {name} already seen)");
            }
            _ => deduped.push(*addr),
        }
    }

    if !deduped.is_empty() {
        tracing::info!("Discovered tephra servers: {:?}", deduped);
    }
    deduped
}

async fn probe_single(client: &reqwest::Client, addr: SocketAddr) -> Option<SocketAddr> {
    let resp = client
        .get(format!("http://{addr}/health"))
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => Some(addr),
        _ => None,
    }
}

fn find_local_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}
