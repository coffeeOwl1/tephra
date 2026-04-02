use std::net::SocketAddr;

use crate::net::api_types::{
    HistoryResponse, Snapshot, SystemInfo, ThrottleEvent, WorkloadEndEvent, WorkloadStartEvent,
};
use crate::node::NodeId;

/// Top-level application message.
#[derive(Debug, Clone)]
pub enum Message {
    /// A new node should be connected (from manual add or discovery).
    AddNode(SocketAddr),

    /// Nodes discovered via subnet scan.
    DiscoveredNodes(Vec<SocketAddr>),

    /// Per-node event, tagged with the node's identity.
    Node(NodeId, NodeMessage),

    /// Navigate to the dashboard view.
    NavigateDashboard,

    /// Navigate to the comparison dashboard.
    NavigateCompare,

    /// Navigate to a node's detail view.
    NavigateDetail(NodeId),

    /// Switch tab in detail view.
    SwitchTab(DetailTab),

    /// Open the add-node dialog.
    OpenAddDialog,

    /// Close the add-node dialog.
    CloseAddDialog,

    /// Text input changed in the add-node dialog.
    AddDialogInput(String),

    /// Submit the add-node dialog.
    AddDialogSubmit,

    /// Remove a node.
    RemoveNode(NodeId),

    /// Retry a failed connection.
    RetryConnection(NodeId),

    /// Set a custom display name for a node (None to clear).
    SetDisplayName(NodeId, Option<String>),

    /// Keyboard event.
    KeyPressed(iced::keyboard::Key),

    /// Open workload detail overlay for the given node.
    OpenWorkloadOverlay,

    /// Close workload overlay.
    CloseWorkloadOverlay,

    /// Navigate workload overlay: previous/next.
    WorkloadOverlayPrev,
    WorkloadOverlayNext,

    /// Set the event console filter in Compare view.
    SetConsoleFilter(EventFilter),

    /// Toggle sort column in Compare summary table.
    SetSummarySort(SummaryColumn),

    /// Navigate to the global settings view.
    NavigateSettings,

    /// Update global alert defaults.
    UpdateAlertDefaults(crate::alerts::AlertDefaults),

    /// Set per-node alert overrides (replaces all overrides for the node).
    SetNodeAlertOverride(NodeId, crate::alerts::AlertOverrides),

    /// Clear all alert overrides for a node (revert to global defaults).
    ClearNodeAlertOverrides(NodeId),

    /// Change the chart history duration.
    SetHistoryDuration(crate::node::history::HistoryDuration),

    /// Send a test desktop notification.
    SendTestNotification,
}

/// Filter for the throttle event console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventFilter {
    #[default]
    All,
    Thermal,
    Power,
}

impl EventFilter {
    pub const ALL: [EventFilter; 3] = [Self::All, Self::Thermal, Self::Power];
}

impl std::fmt::Display for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "All Events"),
            Self::Thermal => write!(f, "Thermal"),
            Self::Power => write!(f, "Power"),
        }
    }
}

/// Sortable columns in the Compare summary table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SummaryColumn {
    Node,
    Cores,
    #[default]
    Temp,
    Peak,
    ClientPeak,
    T95,
    Power,
    PeakPower,
    Freq,
    Util,
    Efficiency,
    Fan,
    Energy,
    Uptime,
    ThrottleTime,
    Throttle,
}

/// Events from a specific node's SSE connection.
#[derive(Debug, Clone)]
pub enum NodeMessage {
    SystemInfoFetched(SystemInfo),
    HistoryFetched(HistoryResponse),
    Connected,
    SnapshotReceived(Snapshot),
    ThrottleEvent(ThrottleEvent),
    WorkloadStart(WorkloadStartEvent),
    WorkloadEnd(WorkloadEndEvent),
    Disconnected(String),
    ConnectionFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Overview,
    Cores,
    Events,
    History,
    Alerts,
}
