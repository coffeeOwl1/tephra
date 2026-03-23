use serde::Serialize;

use crate::monitor::{SystemState, WorkloadSegment};

#[derive(Serialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub cpu_model: String,
    pub core_count: usize,
    pub max_freq_mhz: u32,
    pub scaling_driver: String,
    pub governor: String,
    pub ram_gb: f64,
    pub agent_version: &'static str,
}

#[derive(Serialize, Clone)]
pub struct CoreSnapshot {
    pub freq_mhz: u32,
    pub util_pct: f64,
}

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub timestamp_ms: u128,
    pub temp_c: u32,
    pub temp_rate_cs: i32,
    pub ppt_watts: f64,
    pub avg_freq_mhz: u32,
    pub avg_util_pct: f64,
    pub fan_rpm: u32,
    pub fan_detected: bool,
    pub throttle_active: bool,
    pub throttle_reason: &'static str,
    pub peak_temp: u32,
    pub peak_ppt: f64,
    pub peak_freq: u32,
    pub peak_fan: u32,
    pub thermal_events: u32,
    pub power_events: u32,
    pub energy_wh: f64,
    pub uptime_secs: f64,
    pub cores: Vec<CoreSnapshot>,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub interval_ms: u64,
    pub samples: usize,
    pub temp_c: Vec<u64>,
    pub avg_freq_mhz: Vec<u64>,
    pub ppt_watts: Vec<f64>,
    pub avg_util_pct: Vec<f64>,
    pub fan_rpm: Vec<u64>,
}

#[derive(Serialize, Clone)]
pub struct WorkloadEvent {
    pub id: u32,
    pub start_time: String,
    pub end_time: String,
    pub duration_secs: f64,
    pub peak_temp: u32,
    pub avg_temp: f64,
    pub peak_ppt: f64,
    pub avg_ppt: f64,
    pub energy_wh: f64,
    pub avg_freq: u32,
    pub avg_util: f64,
    pub thermal_events: u32,
    pub power_events: u32,
}

#[derive(Serialize, Clone)]
pub struct ThrottleEvent {
    pub reason: &'static str,
    pub temp_c: u32,
    pub ppt_watts: f64,
}

impl Snapshot {
    pub fn from_state(state: &SystemState) -> Self {
        let cores: Vec<CoreSnapshot> = state
            .cores
            .iter()
            .map(|c| CoreSnapshot {
                freq_mhz: c.freq_mhz,
                util_pct: (c.util_pct * 10.0).round() / 10.0,
            })
            .collect();

        let temp_rate = state.temp_rate();

        Snapshot {
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            temp_c: state.temp_c,
            temp_rate_cs: (temp_rate * 100.0) as i32,
            ppt_watts: (state.ppt_watts * 10.0).round() / 10.0,
            avg_freq_mhz: state.avg_freq(),
            avg_util_pct: (state.avg_util() * 10.0).round() / 10.0,
            fan_rpm: state.fan_rpm,
            fan_detected: state.has_fan_sensor(),
            throttle_active: state.throttle_active,
            throttle_reason: state.throttle_reason.as_str(),
            peak_temp: state.peak_temp,
            peak_ppt: (state.peak_ppt * 10.0).round() / 10.0,
            peak_freq: state.peak_freq,
            peak_fan: state.peak_fan,
            thermal_events: state.thermal_count,
            power_events: state.power_count,
            energy_wh: (state.energy_wh() * 1000.0).round() / 1000.0,
            uptime_secs: (state.start_time.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            cores,
        }
    }
}

impl HistoryResponse {
    pub fn from_state(state: &SystemState, interval_ms: u64) -> Self {
        HistoryResponse {
            interval_ms,
            samples: state.temp_history.len(),
            temp_c: state.temp_history.iter().copied().collect(),
            avg_freq_mhz: state.freq_history.iter().copied().collect(),
            ppt_watts: state
                .ppt_history
                .iter()
                .map(|&v| (v as f64 / 10.0 * 10.0).round() / 10.0)
                .collect(),
            avg_util_pct: state
                .util_history
                .iter()
                .map(|&v| (v as f64 / 10.0 * 10.0).round() / 10.0)
                .collect(),
            fan_rpm: state.fan_history.iter().copied().collect(),
        }
    }
}

fn format_wall_time(t: (u8, u8, u8)) -> String {
    format!("{:02}:{:02}:{:02}", t.0, t.1, t.2)
}

impl WorkloadEvent {
    pub fn from_segment(seg: &WorkloadSegment) -> Self {
        WorkloadEvent {
            id: seg.id,
            start_time: format_wall_time(seg.start_wall),
            end_time: format_wall_time(seg.end_wall),
            duration_secs: (seg.duration_secs * 10.0).round() / 10.0,
            peak_temp: seg.peak_temp,
            avg_temp: (seg.avg_temp * 10.0).round() / 10.0,
            peak_ppt: (seg.peak_ppt * 10.0).round() / 10.0,
            avg_ppt: (seg.avg_ppt * 10.0).round() / 10.0,
            energy_wh: (seg.energy_wh * 1000.0).round() / 1000.0,
            avg_freq: seg.avg_freq,
            avg_util: (seg.avg_util * 10.0).round() / 10.0,
            thermal_events: seg.thermal_events,
            power_events: seg.power_events,
        }
    }
}
