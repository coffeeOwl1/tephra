use std::time::Instant;

use crate::node::NodeState;

/// Global alert defaults — written to [alerts] in config.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct AlertDefaults {
    #[serde(default = "default_temp_ceiling")]
    pub temp_ceiling: i32,
    /// Alias "throttle" migrates old single-field configs to the thermal setting.
    #[serde(default = "default_true", alias = "throttle")]
    pub throttle_thermal: bool,
    #[serde(default)]
    pub throttle_power: bool,
    #[serde(default)]
    pub workload_complete: bool,
    #[serde(default = "default_true")]
    pub connection_lost: bool,
    // Per-alert-type persistence (notification stays until dismissed).
    #[serde(default = "default_true")]
    pub temp_persistent: bool,
    #[serde(default = "default_true")]
    pub throttle_thermal_persistent: bool,
    #[serde(default)]
    pub throttle_power_persistent: bool,
    #[serde(default)]
    pub workload_persistent: bool,
    #[serde(default)]
    pub connection_persistent: bool,
    /// Timeout in seconds for non-persistent notifications.
    #[serde(default = "default_timeout")]
    pub notification_timeout_secs: u32,
}

fn default_temp_ceiling() -> i32 { 85 }
fn default_true() -> bool { true }
fn default_timeout() -> u32 { 5 }

impl Default for AlertDefaults {
    fn default() -> Self {
        Self {
            temp_ceiling: 85,
            throttle_thermal: true,
            throttle_power: false,
            workload_complete: false,
            connection_lost: true,
            temp_persistent: true,
            throttle_thermal_persistent: true,
            throttle_power_persistent: false,
            workload_persistent: false,
            connection_persistent: false,
            notification_timeout_secs: 5,
        }
    }
}

/// Per-node alert overrides. `None` fields inherit from `AlertDefaults`.
#[derive(serde::Deserialize, serde::Serialize, Default, Clone, Debug)]
pub struct AlertOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_ceiling: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle_thermal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle_power: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workload_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_lost: Option<bool>,
}

impl AlertOverrides {
    pub fn has_any(&self) -> bool {
        self.temp_ceiling.is_some()
            || self.throttle_thermal.is_some()
            || self.throttle_power.is_some()
            || self.workload_complete.is_some()
            || self.connection_lost.is_some()
    }
}

/// Resolved alert settings (defaults merged with overrides).
pub struct ResolvedAlerts {
    pub temp_ceiling: i32,
    pub throttle_thermal: bool,
    pub throttle_power: bool,
    pub workload_complete: bool,
    pub connection_lost: bool,
}

pub fn resolve(defaults: &AlertDefaults, overrides: &AlertOverrides) -> ResolvedAlerts {
    ResolvedAlerts {
        temp_ceiling: overrides.temp_ceiling.unwrap_or(defaults.temp_ceiling),
        throttle_thermal: overrides.throttle_thermal.unwrap_or(defaults.throttle_thermal),
        throttle_power: overrides.throttle_power.unwrap_or(defaults.throttle_power),
        workload_complete: overrides.workload_complete.unwrap_or(defaults.workload_complete),
        connection_lost: overrides.connection_lost.unwrap_or(defaults.connection_lost),
    }
}

/// Per-node runtime alert tracking (not persisted).
#[derive(Default)]
pub struct AlertState {
    /// Whether temp is currently above the configured ceiling.
    pub temp_above_ceiling: bool,
    /// When the last temperature notification was sent.
    pub last_temp_notify: Option<Instant>,
    /// How many throttle events we've already notified about.
    pub notified_throttle_count: usize,
    /// How many completed workloads we've already notified about.
    pub notified_workload_count: usize,
    /// Whether the node was connected on the previous evaluation.
    pub was_connected: bool,
}

/// A notification to be sent, with its delivery behavior.
pub struct AlertNotification {
    pub notification: PendingNotification,
    pub persistent: bool,
}

pub enum PendingNotification {
    TempCeiling {
        node_name: String,
        temp: i32,
        ceiling: i32,
        timestamp: chrono::DateTime<chrono::Local>,
    },
    Throttle {
        node_name: String,
        reason: String,
        temp: i32,
        timestamp: chrono::DateTime<chrono::Local>,
    },
    WorkloadComplete {
        node_name: String,
        duration_secs: f64,
        timestamp: chrono::DateTime<chrono::Local>,
    },
    ConnectionLost {
        node_name: String,
        timestamp: chrono::DateTime<chrono::Local>,
    },
}

const TEMP_NOTIFY_COOLDOWN_SECS: u64 = 60;

/// Evaluate alerts for a single node. Returns notifications to send.
pub fn check_alerts(defaults: &AlertDefaults, node: &mut NodeState) -> Vec<AlertNotification> {
    let resolved = resolve(defaults, &node.alert_overrides);
    let mut pending = Vec::new();

    // Temperature ceiling
    if let Some(snap) = &node.snapshot {
        let above = snap.temp_c >= resolved.temp_ceiling;
        let was_above = node.alert_state.temp_above_ceiling;
        let cooldown_ok = node
            .alert_state
            .last_temp_notify
            .map(|t| t.elapsed().as_secs() >= TEMP_NOTIFY_COOLDOWN_SECS)
            .unwrap_or(true);

        if above && !was_above && cooldown_ok {
            pending.push(AlertNotification {
                notification: PendingNotification::TempCeiling {
                    node_name: node.display_name(),
                    temp: snap.temp_c,
                    ceiling: resolved.temp_ceiling,
                    timestamp: chrono::Local::now(),
                },
                persistent: defaults.temp_persistent,
            });
            node.alert_state.last_temp_notify = Some(Instant::now());
        }
        node.alert_state.temp_above_ceiling = above;
    }

    // Throttle events (thermal and power tracked separately)
    {
        let new_count = node.throttle_log.len();
        let old_count = node.alert_state.notified_throttle_count;
        if new_count > old_count {
            for ts_evt in &node.throttle_log[old_count..new_count] {
                let evt = &ts_evt.event;
                let (enabled, persistent) = match evt.reason.as_str() {
                    "thermal" => (resolved.throttle_thermal, defaults.throttle_thermal_persistent),
                    "power" => (resolved.throttle_power, defaults.throttle_power_persistent),
                    _ => (resolved.throttle_thermal, defaults.throttle_thermal_persistent),
                };
                if enabled {
                    pending.push(AlertNotification {
                        notification: PendingNotification::Throttle {
                            node_name: node.display_name(),
                            reason: evt.reason.clone(),
                            temp: evt.temp_c,
                            timestamp: ts_evt.received_at,
                        },
                        persistent,
                    });
                }
            }
            node.alert_state.notified_throttle_count = new_count;
        }
    }

    // Workload complete
    if resolved.workload_complete {
        let new_count = node.completed_workloads.len();
        let old_count = node.alert_state.notified_workload_count;
        if new_count > old_count {
            let wl = &node.completed_workloads[new_count - 1];
            pending.push(AlertNotification {
                notification: PendingNotification::WorkloadComplete {
                    node_name: node.display_name(),
                    duration_secs: wl.duration_secs,
                    timestamp: chrono::Local::now(),
                },
                persistent: defaults.workload_persistent,
            });
            node.alert_state.notified_workload_count = new_count;
        }
    }

    // Connection lost
    if resolved.connection_lost {
        let now_failed = matches!(node.status, crate::node::ConnectionStatus::Failed(_));
        if now_failed && node.alert_state.was_connected {
            pending.push(AlertNotification {
                notification: PendingNotification::ConnectionLost {
                    node_name: node.display_name(),
                    timestamp: chrono::Local::now(),
                },
                persistent: defaults.connection_persistent,
            });
        }
        node.alert_state.was_connected = node.status.is_connected();
    }

    pending
}

/// Send a desktop notification in a background thread.
pub fn send_notification(alert: AlertNotification, timeout_secs: u32) {
    std::thread::spawn(move || {
        let timeout = if alert.persistent {
            notify_rust::Timeout::Never
        } else {
            notify_rust::Timeout::Milliseconds(timeout_secs * 1000)
        };
        let (summary, body) = format_notification(&alert.notification);
        let result = notify_rust::Notification::new()
            .summary(&summary)
            .body(&body)
            .appname("tephra")
            .timeout(timeout)
            .show();
        if let Err(e) = result {
            tracing::warn!("Failed to send desktop notification: {e}");
        }
    });
}

fn format_notification(n: &PendingNotification) -> (String, String) {
    match n {
        PendingNotification::TempCeiling {
            node_name,
            temp,
            ceiling,
            timestamp,
        } => {
            let time = timestamp.format("%-I:%M %p");
            (
                format!("Tephra \u{2014} {node_name}"),
                format!("Temperature {temp}\u{00b0}C exceeded ceiling ({ceiling}\u{00b0}C) at {time}"),
            )
        }
        PendingNotification::Throttle {
            node_name,
            reason,
            temp,
            timestamp,
        } => {
            let time = timestamp.format("%-I:%M %p");
            (
                format!("Tephra \u{2014} {node_name}"),
                format!("{} throttle at {time} \u{2014} {temp}\u{00b0}C", capitalize(reason)),
            )
        }
        PendingNotification::WorkloadComplete {
            node_name,
            duration_secs,
            timestamp,
        } => {
            let time = timestamp.format("%-I:%M %p");
            (
                format!("Tephra \u{2014} {node_name}"),
                format!("Workload completed in {duration_secs:.0}s at {time}"),
            )
        }
        PendingNotification::ConnectionLost {
            node_name,
            timestamp,
        } => {
            let time = timestamp.format("%-I:%M %p");
            (
                format!("Tephra \u{2014} {node_name}"),
                format!("Connection lost at {time}"),
            )
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}
