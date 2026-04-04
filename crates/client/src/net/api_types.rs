#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub cpu_model: String,
    pub core_count: u32,
    pub max_freq_mhz: u32,
    pub scaling_driver: String,
    pub governor: String,
    pub ram_gb: f64,
    pub agent_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreSnapshot {
    pub freq_mhz: u32,
    pub util_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    pub timestamp_ms: u64,
    pub temp_c: i32,
    pub temp_rate_cs: i32,
    pub ppt_watts: f64,
    pub avg_freq_mhz: u32,
    pub avg_util_pct: f64,
    pub fan_rpm: u32,
    pub fan_detected: bool,
    pub throttle_active: bool,
    pub throttle_reason: String,
    pub peak_temp: i32,
    pub peak_ppt: f64,
    pub peak_freq: u32,
    pub peak_fan: u32,
    pub thermal_events: u32,
    pub power_events: u32,
    pub energy_wh: f64,
    pub uptime_secs: f64,
    pub cores: Vec<CoreSnapshot>,
    #[serde(default)]
    pub top_processes: Vec<ProcessSnapshot>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryResponse {
    pub interval_ms: u32,
    pub samples: usize,
    pub temp_c: Vec<i32>,
    pub avg_freq_mhz: Vec<u32>,
    pub ppt_watts: Vec<f64>,
    pub avg_util_pct: Vec<f64>,
    pub fan_rpm: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThrottleEvent {
    pub reason: String,
    pub temp_c: i32,
    pub ppt_watts: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkloadStartEvent {
    pub id: u32,
    pub start_time: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkloadEndEvent {
    pub id: u32,
    pub start_time: String,
    pub end_time: String,
    pub duration_secs: f64,
    pub peak_temp: i32,
    pub avg_temp: f64,
    pub peak_ppt: f64,
    pub avg_ppt: f64,
    pub energy_wh: f64,
    pub avg_freq: u32,
    pub avg_util: f64,
    pub thermal_events: u32,
    pub power_events: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real snapshot payload captured from a tephra-server instance.
    const REAL_SNAPSHOT: &str = r#"{"timestamp_ms":1774072362397,"temp_c":50,"temp_rate_cs":0,"ppt_watts":10.4,"avg_freq_mhz":4495,"avg_util_pct":0.1,"fan_rpm":0,"fan_detected":false,"throttle_active":false,"throttle_reason":"none","peak_temp":83,"peak_ppt":94.0,"peak_freq":4500,"peak_fan":0,"thermal_events":0,"power_events":0,"energy_wh":22.205,"uptime_secs":4017.6,"cores":[{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4464,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":2.0},{"freq_mhz":4466,"util_pct":0.0},{"freq_mhz":4466,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":2.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4466,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0},{"freq_mhz":4500,"util_pct":0.0}]}"#;

    #[test]
    fn parse_real_snapshot() {
        let snap: Snapshot = serde_json::from_str(REAL_SNAPSHOT).unwrap();
        assert_eq!(snap.temp_c, 50);
        assert_eq!(snap.avg_freq_mhz, 4495);
        assert_eq!(snap.cores.len(), 32);
        assert!(!snap.throttle_active);
        assert_eq!(snap.throttle_reason, "none");
        assert_eq!(snap.peak_temp, 83);
    }

    #[test]
    fn parse_system_info() {
        let json = r#"{"hostname":"pantheon","cpu_model":"AMD Ryzen 9 7945HX with Radeon Graphics","core_count":32,"max_freq_mhz":5462,"scaling_driver":"amd-pstate","governor":"performance","ram_gb":28.6,"agent_version":"0.1.0"}"#;
        let info: SystemInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.hostname, "pantheon");
        assert_eq!(info.core_count, 32);
        assert_eq!(info.max_freq_mhz, 5462);
    }

    #[test]
    fn parse_history_response() {
        let json = r#"{"interval_ms":500,"samples":3,"temp_c":[50,51,52],"avg_freq_mhz":[4500,4490,4495],"ppt_watts":[10.0,11.0,10.5],"avg_util_pct":[1.0,2.0,1.5],"fan_rpm":[0,0,0]}"#;
        let hist: HistoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(hist.samples, 3);
        assert_eq!(hist.temp_c.len(), 3);
        assert_eq!(hist.ppt_watts[1], 11.0);
    }

    #[test]
    fn parse_throttle_event() {
        let json = r#"{"reason":"thermal","temp_c":92,"ppt_watts":125.0}"#;
        let evt: ThrottleEvent = serde_json::from_str(json).unwrap();
        assert_eq!(evt.reason, "thermal");
        assert_eq!(evt.temp_c, 92);
    }

    #[test]
    fn parse_workload_events() {
        let start_json = r#"{"id":1,"start_time":"14:30:05"}"#;
        let start: WorkloadStartEvent = serde_json::from_str(start_json).unwrap();
        assert_eq!(start.id, 1);

        let end_json = r#"{"id":1,"start_time":"14:30:05","end_time":"14:31:20","duration_secs":75.0,"peak_temp":82,"avg_temp":76.5,"peak_ppt":110.2,"avg_ppt":85.4,"energy_wh":1.78,"avg_freq":4800,"avg_util":88.3,"thermal_events":0,"power_events":0}"#;
        let end: WorkloadEndEvent = serde_json::from_str(end_json).unwrap();
        assert_eq!(end.duration_secs, 75.0);
        assert_eq!(end.peak_temp, 82);
    }

    #[test]
    fn snapshot_handles_all_zero_values() {
        let json = r#"{"timestamp_ms":0,"temp_c":0,"temp_rate_cs":0,"ppt_watts":0.0,"avg_freq_mhz":0,"avg_util_pct":0.0,"fan_rpm":0,"fan_detected":false,"throttle_active":false,"throttle_reason":"none","peak_temp":0,"peak_ppt":0.0,"peak_freq":0,"peak_fan":0,"thermal_events":0,"power_events":0,"energy_wh":0.0,"uptime_secs":0.0,"cores":[]}"#;
        let snap: Snapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snap.cores.len(), 0);
    }
}
