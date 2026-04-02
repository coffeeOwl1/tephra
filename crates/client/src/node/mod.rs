pub mod connection;
pub mod history;

use std::net::SocketAddr;
use std::time::Instant;

use iced::widget::canvas;
use uuid::Uuid;

use crate::net::api_types::{
    Snapshot, SystemInfo, ThrottleEvent, WorkloadEndEvent, WorkloadStartEvent,
};
use history::TimeSeriesStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    FetchingInfo,
    Streaming,
    Reconnecting(u32),
    #[allow(dead_code)]
    Failed(String),
}

#[allow(dead_code)]
impl ConnectionStatus {
    pub fn label(&self) -> &str {
        match self {
            Self::Connecting => "Connecting",
            Self::FetchingInfo => "Fetching info",
            Self::Streaming => "Connected",
            Self::Reconnecting(_) => "Reconnecting",
            Self::Failed(_) => "Failed",
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Streaming)
    }
}

/// Canvas caches for charts. Cleared when new data arrives so charts redraw.
pub struct ChartCaches {
    pub temp: canvas::Cache,
    pub power: canvas::Cache,
    pub freq: canvas::Cache,
    pub util: canvas::Cache,
    pub fan: canvas::Cache,
    pub sparkline: canvas::Cache,
    pub core_grid: canvas::Cache,
}

impl ChartCaches {
    pub fn new() -> Self {
        Self {
            temp: canvas::Cache::new(),
            power: canvas::Cache::new(),
            freq: canvas::Cache::new(),
            util: canvas::Cache::new(),
            fan: canvas::Cache::new(),
            sparkline: canvas::Cache::new(),
            core_grid: canvas::Cache::new(),
        }
    }

    pub fn clear_all(&self) {
        self.temp.clear();
        self.power.clear();
        self.freq.clear();
        self.util.clear();
        self.fan.clear();
        self.sparkline.clear();
        self.core_grid.clear();
    }
}

/// A throttle event with the wall-clock time it was received by the client.
#[derive(Debug, Clone)]
pub struct TimestampedThrottle {
    pub event: ThrottleEvent,
    pub received_at: chrono::DateTime<chrono::Local>,
}

#[derive(Debug, Clone)]
pub struct ActiveWorkload {
    pub id: u32,
    pub start_time: String,
    /// Whether this workload was started by a server SSE event (vs client-side detection).
    /// Server-managed workloads are only ended by server `workload_end` events.
    pub server_managed: bool,
}

/// All state for a single tephra-server node.
pub struct NodeState {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub status: ConnectionStatus,
    /// User-assigned display name (overrides hostname).
    pub custom_name: Option<String>,
    pub system_info: Option<SystemInfo>,
    /// Warning message for version incompatibility.
    pub version_warning: Option<String>,
    pub snapshot: Option<Snapshot>,
    pub history: TimeSeriesStore,
    pub throttle_log: Vec<TimestampedThrottle>,
    pub completed_workloads: Vec<WorkloadEndEvent>,
    /// Counts at last detail view visit (for notification badges).
    pub last_viewed_throttle_count: usize,
    pub last_viewed_workload_count: usize,
    pub active_workload: Option<ActiveWorkload>,
    pub last_update: Option<Instant>,
    pub caches: ChartCaches,
    /// Efficiency baseline (MHz/W) captured at first sample where PPT > 5W.
    pub efficiency_baseline: Option<f64>,
    /// Cumulative throttle time in 500ms ticks.
    pub throttle_ticks: u32,
    /// Was throttling active on the previous snapshot?
    prev_throttle: bool,
    /// Instant of last throttle state transition (for flash animation).
    pub throttle_changed_at: Option<Instant>,
    /// Reason from the last throttle event (persists after throttle ends for lingering badge).
    pub last_throttle_reason: String,
    /// When throttle ended (for lingering dimmed badge).
    pub throttle_ended_at: Option<Instant>,
    /// Temperature duration: ticks spent at exactly each degree [0..=105].
    pub temp_duration_ticks: [u32; 106],
    /// Current continuous streak at or above each degree [0..=105].
    pub temp_streak_current: [u32; 106],
    /// Longest continuous streak at or above each degree [0..=105].
    pub temp_streak_max: [u32; 106],
    /// Ticks of low utilization while a workload is active (for stale detection).
    workload_low_util_ticks: u32,
    /// Client-side workload detection: ticks of high utilization before workload starts.
    workload_high_util_ticks: u32,
    /// Client-side workload tracking: snapshot count during active workload.
    workload_snap_count: u32,
    /// Running averages for client-side workload stats.
    workload_sum_temp: f64,
    workload_sum_ppt: f64,
    workload_sum_util: f64,
    workload_sum_freq: f64,
    workload_peak_temp: i32,
    workload_peak_ppt: f64,
    workload_energy_start: f64,
    /// Throttle events during current workload.
    workload_thermal_events: u32,
    workload_power_events: u32,
    /// Next workload ID.
    next_workload_id: u32,
    /// Per-node alert overrides (None fields inherit from global defaults).
    pub alert_overrides: crate::alerts::AlertOverrides,
    /// Runtime alert tracking state (not persisted).
    pub alert_state: crate::alerts::AlertState,
}

impl NodeState {
    #[allow(dead_code)]
    pub fn new(id: NodeId, addr: SocketAddr) -> Self {
        Self::with_capacity(id, addr, TimeSeriesStore::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(id: NodeId, addr: SocketAddr, history_capacity: usize) -> Self {
        Self {
            id,
            addr,
            status: ConnectionStatus::Connecting,
            custom_name: None,
            system_info: None,
            version_warning: None,
            snapshot: None,
            history: TimeSeriesStore::with_capacity(history_capacity),
            throttle_log: Vec::new(),
            completed_workloads: Vec::new(),
            last_viewed_throttle_count: 0,
            last_viewed_workload_count: 0,
            active_workload: None,
            last_update: None,
            caches: ChartCaches::new(),
            efficiency_baseline: None,
            throttle_ticks: 0,
            prev_throttle: false,
            throttle_changed_at: None,
            last_throttle_reason: String::new(),
            throttle_ended_at: None,
            temp_duration_ticks: [0; 106],
            temp_streak_current: [0; 106],
            temp_streak_max: [0; 106],
            workload_low_util_ticks: 0,
            workload_high_util_ticks: 0,
            workload_snap_count: 0,
            workload_sum_temp: 0.0,
            workload_sum_ppt: 0.0,
            workload_sum_util: 0.0,
            workload_sum_freq: 0.0,
            workload_peak_temp: 0,
            workload_peak_ppt: 0.0,
            workload_energy_start: 0.0,
            workload_thermal_events: 0,
            workload_power_events: 0,
            next_workload_id: 1,
            alert_overrides: Default::default(),
            alert_state: Default::default(),
        }
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = &self.custom_name {
            return name.clone();
        }
        self.system_info
            .as_ref()
            .map(|info| info.hostname.clone())
            .unwrap_or_else(|| self.addr.to_string())
    }

    pub fn on_snapshot(&mut self, snap: Snapshot) {
        // Capture efficiency baseline on first meaningful power reading
        if self.efficiency_baseline.is_none() && snap.ppt_watts > 5.0 && snap.avg_freq_mhz > 0 {
            self.efficiency_baseline = Some(snap.avg_freq_mhz as f64 / snap.ppt_watts);
        }
        // Track throttle state transitions
        if snap.throttle_active != self.prev_throttle {
            self.throttle_changed_at = Some(Instant::now());
            if snap.throttle_active {
                // Rising edge: record reason, clear ended timestamp
                self.last_throttle_reason = snap.throttle_reason.clone();
                self.throttle_ended_at = None;
                // Track per-workload throttle events
                if self.active_workload.is_some() {
                    if snap.throttle_reason == "thermal" {
                        self.workload_thermal_events += 1;
                    } else {
                        self.workload_power_events += 1;
                    }
                }
            } else {
                // Falling edge: record when throttle ended
                self.throttle_ended_at = Some(Instant::now());
            }
        }
        // Accumulate throttle time
        if snap.throttle_active {
            self.throttle_ticks += 1;
        }
        self.prev_throttle = snap.throttle_active;

        // Client-side workload detection
        self.detect_workload(&snap);

        // Temperature duration tracking (matches reference implementation)
        let temp_idx = (snap.temp_c as usize).min(105);
        // Duration: count at exactly this degree
        self.temp_duration_ticks[temp_idx] = self.temp_duration_ticks[temp_idx].saturating_add(1);
        // Streaks: "at or above" semantics for all degrees
        for t in 0..=105 {
            if temp_idx >= t {
                self.temp_streak_current[t] = self.temp_streak_current[t].saturating_add(1);
                if self.temp_streak_current[t] > self.temp_streak_max[t] {
                    self.temp_streak_max[t] = self.temp_streak_current[t];
                }
            } else {
                self.temp_streak_current[t] = 0;
            }
        }

        self.history.push_snapshot(&snap);
        self.snapshot = Some(snap);
        self.last_update = Some(Instant::now());
        // Note: cache clearing is done selectively by the update handler
        // based on which view is currently active.
    }

    /// Client-side workload detection.
    /// Start: avg util ≥ 25% for 10 ticks (5s). End: avg util < 15% for 10 ticks (5s).
    fn detect_workload(&mut self, snap: &Snapshot) {
        const START_UTIL: f64 = 25.0;
        const END_UTIL: f64 = 15.0;
        const START_TICKS: u32 = 10;
        const END_TICKS: u32 = 10;

        if let Some(active) = &self.active_workload {
            let server_managed = active.server_managed;

            // Track stats during active workload (regardless of who started it).
            self.workload_snap_count += 1;
            self.workload_sum_temp += snap.temp_c as f64;
            self.workload_sum_ppt += snap.ppt_watts;
            self.workload_sum_util += snap.avg_util_pct;
            self.workload_sum_freq += snap.avg_freq_mhz as f64;
            self.workload_peak_temp = self.workload_peak_temp.max(snap.temp_c);
            if snap.ppt_watts > self.workload_peak_ppt {
                self.workload_peak_ppt = snap.ppt_watts;
            }

            // Only client-detected workloads are ended by client-side detection.
            // Server-managed workloads wait for the server's workload_end event.
            if !server_managed {
                if snap.avg_util_pct < END_UTIL {
                    self.workload_low_util_ticks += 1;
                    if self.workload_low_util_ticks >= END_TICKS {
                        self.finish_workload(snap);
                    }
                } else {
                    self.workload_low_util_ticks = 0;
                }
            }
        } else {
            // Check for start (only if the server hasn't started one already).
            if snap.avg_util_pct >= START_UTIL {
                self.workload_high_util_ticks += 1;
                if self.workload_high_util_ticks >= START_TICKS {
                    self.start_workload(snap);
                }
            } else {
                self.workload_high_util_ticks = 0;
            }
        }
    }

    fn start_workload(&mut self, snap: &Snapshot) {
        let id = self.next_workload_id;
        self.next_workload_id += 1;

        // Format time from uptime
        let secs = snap.uptime_secs as u64;
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        let start_time = format!("{h:02}:{m:02}:{s:02}");

        self.active_workload = Some(ActiveWorkload { id, start_time, server_managed: false });
        self.workload_snap_count = 0;
        self.workload_sum_temp = 0.0;
        self.workload_sum_ppt = 0.0;
        self.workload_sum_util = 0.0;
        self.workload_sum_freq = 0.0;
        self.workload_peak_temp = snap.temp_c;
        self.workload_peak_ppt = snap.ppt_watts;
        self.workload_energy_start = snap.energy_wh;
        self.workload_thermal_events = 0;
        self.workload_power_events = 0;
        self.workload_low_util_ticks = 0;
        self.workload_high_util_ticks = 0;
    }

    fn finish_workload(&mut self, snap: &Snapshot) {
        let active = match self.active_workload.take() {
            Some(a) => a,
            None => return,
        };

        let n = self.workload_snap_count.max(1) as f64;
        let duration = n * 0.5; // Each tick is 500ms

        let secs = snap.uptime_secs as u64;
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        let end_time = format!("{h:02}:{m:02}:{s:02}");

        let wl = WorkloadEndEvent {
            id: active.id,
            start_time: active.start_time,
            end_time,
            duration_secs: duration,
            peak_temp: self.workload_peak_temp,
            avg_temp: self.workload_sum_temp / n,
            peak_ppt: self.workload_peak_ppt,
            avg_ppt: self.workload_sum_ppt / n,
            energy_wh: snap.energy_wh - self.workload_energy_start,
            avg_freq: (self.workload_sum_freq / n) as u32,
            avg_util: self.workload_sum_util / n,
            thermal_events: self.workload_thermal_events,
            power_events: self.workload_power_events,
        };

        self.completed_workloads.push(wl);
        self.workload_low_util_ticks = 0;
        self.workload_high_util_ticks = 0;
    }

    /// Count of cores with utilization > 20%.
    pub fn busy_core_count(&self) -> usize {
        self.snapshot
            .as_ref()
            .map(|s| s.cores.iter().filter(|c| c.util_pct > 20.0).count())
            .unwrap_or(0)
    }

    /// Per-core power: PPT / busy_core_count.
    pub fn per_core_power(&self) -> Option<f64> {
        let snap = self.snapshot.as_ref()?;
        let busy = self.busy_core_count();
        if busy > 0 && snap.ppt_watts > 0.1 {
            Some(snap.ppt_watts / busy as f64)
        } else {
            None
        }
    }

    /// Cumulative time at or above a given temperature (sums duration_ticks from temp to 105).
    pub fn cumulative_temp_secs(&self, temp: i32) -> f64 {
        let from = (temp as usize).min(105);
        let ticks: u32 = self.temp_duration_ticks[from..=105].iter().sum();
        ticks as f64 * 0.5
    }

    /// Longest continuous streak at or above a given temperature.
    pub fn longest_streak_secs(&self, temp: i32) -> f64 {
        let idx = (temp as usize).min(105);
        self.temp_streak_max[idx] as f64 * 0.5
    }

    /// Current ongoing streak at or above a given temperature.
    pub fn current_streak_secs(&self, temp: i32) -> f64 {
        let idx = (temp as usize).min(105);
        self.temp_streak_current[idx] as f64 * 0.5
    }

    /// Cumulative throttle time in seconds.
    pub fn throttle_secs(&self) -> f64 {
        self.throttle_ticks as f64 * 0.5
    }

    /// Current efficiency and % change from baseline.
    pub fn efficiency_delta(&self) -> Option<(f64, f64)> {
        let snap = self.snapshot.as_ref()?;
        if snap.ppt_watts < 0.1 {
            return None;
        }
        let current = snap.avg_freq_mhz as f64 / snap.ppt_watts;
        let baseline = self.efficiency_baseline?;
        if baseline <= 0.0 {
            return None;
        }
        let pct_change = (current - baseline) / baseline * 100.0;
        Some((current, pct_change))
    }

    pub fn on_throttle(&mut self, evt: ThrottleEvent) {
        self.throttle_log.push(TimestampedThrottle {
            event: evt,
            received_at: chrono::Local::now(),
        });
    }

    pub fn on_workload_start(&mut self, evt: WorkloadStartEvent) {
        self.active_workload = Some(ActiveWorkload {
            id: evt.id,
            start_time: evt.start_time,
            server_managed: true,
        });
        // Reset all client-side tracking so stats accumulate fresh for this workload.
        self.workload_snap_count = 0;
        self.workload_sum_temp = 0.0;
        self.workload_sum_ppt = 0.0;
        self.workload_sum_util = 0.0;
        self.workload_sum_freq = 0.0;
        self.workload_peak_temp = 0;
        self.workload_peak_ppt = 0.0;
        self.workload_energy_start = self.snapshot.as_ref().map_or(0.0, |s| s.energy_wh);
        self.workload_thermal_events = 0;
        self.workload_power_events = 0;
        self.workload_low_util_ticks = 0;
        self.workload_high_util_ticks = 0;
    }

    pub fn on_workload_end(&mut self, evt: WorkloadEndEvent) {
        self.active_workload = None;
        self.completed_workloads.push(evt);
        // Reset detection state so client-side detection doesn't carry over stale ticks.
        self.workload_low_util_ticks = 0;
        self.workload_high_util_ticks = 0;
    }

    /// Reset all client-side accumulated state.
    pub fn reset_client_state(&mut self) {
        self.efficiency_baseline = None;
        self.throttle_ticks = 0;
        self.prev_throttle = false;
        self.throttle_changed_at = None;
        self.temp_duration_ticks = [0; 106];
        self.temp_streak_current = [0; 106];
        self.temp_streak_max = [0; 106];
        self.workload_thermal_events = 0;
        self.workload_power_events = 0;
    }

    /// Number of unread events since last detail view visit.
    pub fn unread_event_count(&self) -> usize {
        let new_throttle = self
            .throttle_log
            .len()
            .saturating_sub(self.last_viewed_throttle_count);
        let new_workloads = self
            .completed_workloads
            .len()
            .saturating_sub(self.last_viewed_workload_count);
        new_throttle + new_workloads
    }

    /// Mark current event counts as "seen".
    pub fn mark_events_viewed(&mut self) {
        self.last_viewed_throttle_count = self.throttle_log.len();
        self.last_viewed_workload_count = self.completed_workloads.len();
    }

    /// Highest temperature the client has actually observed this session (from temp_duration_ticks).
    pub fn client_peak_temp(&self) -> i32 {
        for t in (0..=105).rev() {
            if self.temp_duration_ticks[t] > 0 {
                return t as i32;
            }
        }
        0
    }

    /// Elapsed workload time in seconds (snap_count * 0.5).
    pub fn workload_elapsed_secs(&self) -> f64 {
        self.workload_snap_count as f64 * 0.5
    }

    /// Whether the throttle badge should flash (within 2s of a state transition).
    pub fn is_throttle_flashing(&self) -> bool {
        self.throttle_changed_at
            .is_some_and(|t| t.elapsed().as_secs_f64() < 2.0)
    }

    /// Whether a recently-ended throttle event should still be shown (dimmed).
    /// Returns true for 4 seconds after throttle ends.
    pub fn is_throttle_lingering(&self) -> bool {
        self.throttle_ended_at
            .is_some_and(|t| t.elapsed().as_secs_f64() < 4.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::api_types::{CoreSnapshot, Snapshot};

    fn test_addr() -> SocketAddr {
        "127.0.0.1:9867".parse().unwrap()
    }

    fn make_snap(temp: i32, ppt: f64, freq: u32, util: f64, throttle: bool) -> Snapshot {
        Snapshot {
            timestamp_ms: 0,
            temp_c: temp,
            temp_rate_cs: 0,
            ppt_watts: ppt,
            avg_freq_mhz: freq,
            avg_util_pct: util,
            fan_rpm: 0,
            fan_detected: false,
            throttle_active: throttle,
            throttle_reason: if throttle {
                "thermal".to_string()
            } else {
                String::new()
            },
            peak_temp: temp,
            peak_ppt: ppt,
            peak_freq: freq,
            peak_fan: 0,
            thermal_events: 0,
            power_events: 0,
            energy_wh: 0.0,
            uptime_secs: 0.0,
            cores: vec![
                CoreSnapshot { freq_mhz: freq, util_pct: util },
            ],
            top_processes: vec![],
        }
    }

    // T0-1: Throttle tracking
    #[test]
    fn throttle_ticks_increment_only_when_active() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // Non-throttled snapshot
        node.on_snapshot(make_snap(65, 30.0, 4000, 50.0, false));
        assert_eq!(node.throttle_ticks, 0);
        // Throttled snapshot
        node.on_snapshot(make_snap(95, 80.0, 3000, 90.0, true));
        assert_eq!(node.throttle_ticks, 1);
        // Another throttled snapshot
        node.on_snapshot(make_snap(96, 82.0, 2900, 92.0, true));
        assert_eq!(node.throttle_ticks, 2);
        // Back to normal
        node.on_snapshot(make_snap(70, 40.0, 4000, 50.0, false));
        assert_eq!(node.throttle_ticks, 2); // stays at 2
        // Verify seconds conversion
        assert!((node.throttle_secs() - 1.0).abs() < f64::EPSILON);
    }

    // T0-2: Temp duration at-exactly
    #[test]
    fn temp_duration_increments_at_exact_degree() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        node.on_snapshot(make_snap(75, 30.0, 4000, 50.0, false));
        node.on_snapshot(make_snap(75, 30.0, 4000, 50.0, false));
        node.on_snapshot(make_snap(80, 30.0, 4000, 50.0, false));

        assert_eq!(node.temp_duration_ticks[75], 2);
        assert_eq!(node.temp_duration_ticks[80], 1);
        assert_eq!(node.temp_duration_ticks[74], 0); // not at 74
        assert_eq!(node.temp_duration_ticks[76], 0); // not at 76
    }

    // T0-3: Temp streak at-or-above
    #[test]
    fn temp_streak_tracks_at_or_above() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // 3 ticks at 80°C
        for _ in 0..3 {
            node.on_snapshot(make_snap(80, 30.0, 4000, 50.0, false));
        }
        // Streak at 80 should be 3, at 81 should be 0
        assert_eq!(node.temp_streak_current[80], 3);
        assert_eq!(node.temp_streak_current[81], 0);
        // All degrees <= 80 should have streak 3
        assert_eq!(node.temp_streak_current[70], 3);
        assert_eq!(node.temp_streak_current[0], 3);

        // Drop to 75, streak at 80 resets, max preserved
        node.on_snapshot(make_snap(75, 30.0, 4000, 50.0, false));
        assert_eq!(node.temp_streak_current[80], 0);
        assert_eq!(node.temp_streak_max[80], 3); // max preserved
        // Streak at 75 continues (was at-or-above 75 for all 4 snapshots)
        assert_eq!(node.temp_streak_current[75], 4);
    }

    // T0-4: Cumulative temp secs
    #[test]
    fn cumulative_temp_secs_sums_above() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // 2 ticks at 80, 3 ticks at 85
        node.on_snapshot(make_snap(80, 30.0, 4000, 50.0, false));
        node.on_snapshot(make_snap(80, 30.0, 4000, 50.0, false));
        for _ in 0..3 {
            node.on_snapshot(make_snap(85, 30.0, 4000, 50.0, false));
        }
        // cumulative at 80 = ticks at 80 + ticks at 85 = 2 + 3 = 5 ticks = 2.5s
        assert!((node.cumulative_temp_secs(80) - 2.5).abs() < f64::EPSILON);
        // cumulative at 85 = 3 ticks = 1.5s
        assert!((node.cumulative_temp_secs(85) - 1.5).abs() < f64::EPSILON);
        // cumulative at 90 = 0
        assert!((node.cumulative_temp_secs(90)).abs() < f64::EPSILON);
    }

    // T0-5: Efficiency baseline capture
    #[test]
    fn efficiency_baseline_captured_once() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // Low power — no baseline
        node.on_snapshot(make_snap(50, 2.0, 1000, 10.0, false));
        assert!(node.efficiency_baseline.is_none());

        // First meaningful power reading — captures baseline
        node.on_snapshot(make_snap(70, 60.0, 4500, 80.0, false));
        let baseline = node.efficiency_baseline.unwrap();
        assert!((baseline - 4500.0 / 60.0).abs() < f64::EPSILON);

        // Second high-power snap — baseline NOT overwritten
        node.on_snapshot(make_snap(75, 80.0, 4000, 90.0, false));
        assert!((node.efficiency_baseline.unwrap() - baseline).abs() < f64::EPSILON);
    }

    // T0-6: Efficiency delta computation
    #[test]
    fn efficiency_delta_correct() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // No snapshot → None
        assert!(node.efficiency_delta().is_none());

        // Establish baseline: 4500 MHz / 60W = 75 MHz/W
        node.on_snapshot(make_snap(70, 60.0, 4500, 80.0, false));

        // Same efficiency → ~0% delta
        let (eff, delta) = node.efficiency_delta().unwrap();
        assert!((eff - 75.0).abs() < 0.1);
        assert!(delta.abs() < 0.1);

        // Lower efficiency: 3000 MHz / 60W = 50 MHz/W → -33.3%
        node.on_snapshot(make_snap(80, 60.0, 3000, 90.0, false));
        let (eff, delta) = node.efficiency_delta().unwrap();
        assert!((eff - 50.0).abs() < 0.1);
        assert!((delta - (-33.33)).abs() < 0.5);

        // Near-zero power → None
        node.on_snapshot(make_snap(40, 0.05, 800, 5.0, false));
        assert!(node.efficiency_delta().is_none());
    }

    // T0-7: Per-core power
    #[test]
    fn per_core_power_divides_by_busy_count() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // 4 cores: 2 busy (>20%), 2 idle
        let snap = Snapshot {
            cores: vec![
                CoreSnapshot { freq_mhz: 4000, util_pct: 90.0 },
                CoreSnapshot { freq_mhz: 3500, util_pct: 50.0 },
                CoreSnapshot { freq_mhz: 1000, util_pct: 5.0 },
                CoreSnapshot { freq_mhz: 800, util_pct: 2.0 },
            ],
            ppt_watts: 80.0,
            ..make_snap(70, 80.0, 3000, 40.0, false)
        };
        node.on_snapshot(snap);

        assert_eq!(node.busy_core_count(), 2);
        let pcw = node.per_core_power().unwrap();
        assert!((pcw - 40.0).abs() < f64::EPSILON); // 80W / 2 cores

        // All cores idle → None
        let snap = Snapshot {
            cores: vec![
                CoreSnapshot { freq_mhz: 800, util_pct: 5.0 },
                CoreSnapshot { freq_mhz: 800, util_pct: 3.0 },
            ],
            ppt_watts: 10.0,
            ..make_snap(40, 10.0, 800, 4.0, false)
        };
        node.on_snapshot(snap);
        assert_eq!(node.busy_core_count(), 0);
        assert!(node.per_core_power().is_none());
    }

    // T0-8: Workload start detection
    #[test]
    fn workload_starts_after_10_high_util_ticks() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        assert!(node.active_workload.is_none());

        // 9 ticks at 30% util — not enough
        for _ in 0..9 {
            node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
        }
        assert!(node.active_workload.is_none());

        // 10th tick — workload starts
        node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
        assert!(node.active_workload.is_some());
        assert_eq!(node.active_workload.as_ref().unwrap().id, 1);

        // Drop below threshold resets counter for next workload
        // First finish current workload
        for _ in 0..10 {
            node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false));
        }
        assert!(node.active_workload.is_none());
        assert_eq!(node.completed_workloads.len(), 1);

        // Partial high util, then drop — counter resets
        for _ in 0..5 {
            node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
        }
        node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false)); // reset
        for _ in 0..9 {
            node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
        }
        assert!(node.active_workload.is_none()); // only 9 after reset
    }

    // T0-9: Workload end detection
    #[test]
    fn workload_ends_after_10_low_util_ticks() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // Start a workload
        for _ in 0..10 {
            node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
        }
        assert!(node.active_workload.is_some());

        // 9 ticks of low util — not enough
        for _ in 0..9 {
            node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false));
        }
        assert!(node.active_workload.is_some()); // still active

        // 10th tick — workload ends
        node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false));
        assert!(node.active_workload.is_none());
        assert_eq!(node.completed_workloads.len(), 1);
        let wl = &node.completed_workloads[0];
        assert_eq!(wl.id, 1);
        assert!(wl.duration_secs > 0.0);

        // Recovery resets counter
        for _ in 0..10 {
            node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
        }
        assert!(node.active_workload.is_some());
        // 5 low, then recovery, then 5 low — should NOT end
        for _ in 0..5 {
            node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false));
        }
        node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false)); // recovery
        for _ in 0..5 {
            node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false));
        }
        assert!(node.active_workload.is_some()); // still active
    }

    // T0-10: Workload stats accumulation
    #[test]
    fn workload_stats_computed_correctly() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        // Start workload with 10 ticks at specific values
        for i in 0..10 {
            let mut snap = make_snap(70, 50.0, 4000, 30.0, false);
            snap.energy_wh = i as f64 * 0.01; // increasing energy
            node.on_snapshot(snap);
        }
        assert!(node.active_workload.is_some());

        // Run 10 ticks with known values during active workload
        for i in 0..10 {
            let mut snap = make_snap(80 + (i % 3), 60.0 + i as f64, 4200, 75.0, false);
            snap.energy_wh = 0.1 + (i as f64 * 0.005);
            node.on_snapshot(snap);
        }

        // End workload with 10 low-util ticks
        let mut end_snap = make_snap(50, 20.0, 3000, 10.0, false);
        end_snap.energy_wh = 0.2;
        for _ in 0..10 {
            node.on_snapshot(end_snap.clone());
        }

        assert_eq!(node.completed_workloads.len(), 1);
        let wl = &node.completed_workloads[0];
        assert!(wl.avg_temp > 0.0);
        assert!(wl.avg_ppt > 0.0);
        assert!(wl.avg_freq > 0);
        assert!(wl.avg_util > 0.0);
        assert!(wl.duration_secs > 0.0);
        assert!(wl.peak_temp >= 80);
    }

    // T0-11: Workload ID incrementing
    #[test]
    fn workload_ids_increment() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        for wl_num in 1..=3 {
            // Start
            for _ in 0..10 {
                node.on_snapshot(make_snap(70, 50.0, 4000, 30.0, false));
            }
            assert!(node.active_workload.is_some());
            assert_eq!(node.active_workload.as_ref().unwrap().id, wl_num);
            // End
            for _ in 0..10 {
                node.on_snapshot(make_snap(50, 20.0, 3000, 10.0, false));
            }
        }
        assert_eq!(node.completed_workloads.len(), 3);
        assert_eq!(node.completed_workloads[0].id, 1);
        assert_eq!(node.completed_workloads[1].id, 2);
        assert_eq!(node.completed_workloads[2].id, 3);
    }

    // T0-12: Server workload events
    #[test]
    fn server_workload_events() {
        let mut node = NodeState::new(NodeId::new(), test_addr());
        assert!(node.active_workload.is_none());
        assert!(node.completed_workloads.is_empty());

        // Server start event
        node.on_workload_start(crate::net::api_types::WorkloadStartEvent {
            id: 42,
            start_time: "01:02:03".into(),
        });
        assert!(node.active_workload.is_some());
        assert_eq!(node.active_workload.as_ref().unwrap().id, 42);

        // Server end event
        let end_evt = crate::net::api_types::WorkloadEndEvent {
            id: 42,
            start_time: "01:02:03".into(),
            end_time: "01:03:04".into(),
            duration_secs: 61.0,
            peak_temp: 90,
            avg_temp: 85.0,
            peak_ppt: 100.0,
            avg_ppt: 80.0,
            energy_wh: 1.5,
            avg_freq: 4200,
            avg_util: 90.0,
            thermal_events: 2,
            power_events: 1,
        };
        node.on_workload_end(end_evt);
        assert!(node.active_workload.is_none());
        assert_eq!(node.completed_workloads.len(), 1);
        assert_eq!(node.completed_workloads[0].id, 42);
        assert_eq!(node.completed_workloads[0].thermal_events, 2);
    }

    // T0-13: Display name priority
    #[test]
    fn display_name_priority() {
        let addr = test_addr();
        let mut node = NodeState::new(NodeId::new(), addr);
        // No system_info, no custom_name → addr
        assert_eq!(node.display_name(), addr.to_string());

        // With system_info → hostname
        node.system_info = Some(crate::net::api_types::SystemInfo {
            hostname: "myhost".into(),
            cpu_model: "Test CPU".into(),
            core_count: 4,
            max_freq_mhz: 5000,
            scaling_driver: "acpi".into(),
            governor: "performance".into(),
            ram_gb: 16.0,
            agent_version: "0.1.0".into(),
        });
        assert_eq!(node.display_name(), "myhost");

        // With custom_name → custom_name wins
        node.custom_name = Some("Workstation".into());
        assert_eq!(node.display_name(), "Workstation");
    }
}
