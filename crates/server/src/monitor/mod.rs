use std::collections::VecDeque;
use std::time::Instant;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("tephra-server only supports Linux and Windows");

// Re-export platform-specific free functions
pub use platform::{get_cpu_model, get_governor, get_hostname, get_max_freq, get_scaling_driver,
    get_total_ram_gb, wall_clock_hms};

pub const HISTORY_LEN: usize = 120;
pub const TEMP_OK: u32 = 70;
pub const TEMP_WARM: u32 = 80;
pub const TEMP_HOT: u32 = 90;
pub const TEMP_CRITICAL: u32 = 95;
pub const THROTTLE_UTIL_THRESH: f64 = 30.0;
pub const THROTTLE_FREQ_RATIO: f64 = 0.60;
pub const THERMAL_TEMP_THRESH: u32 = 85;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThrottleReason {
    None,
    Thermal,
    Power,
}

impl ThrottleReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Thermal => "thermal",
            Self::Power => "power",
        }
    }
}

#[derive(Clone)]
pub struct ThermalEvent {
    pub wall_time: (u8, u8, u8),
    pub temp_c: u32,
    pub ppt_watts: f64,
    pub reason: ThrottleReason,
}

pub struct CoreInfo {
    pub freq_mhz: u32,
    pub util_pct: f64,
    pub(super) prev_idle: u64,
    pub(super) prev_total: u64,
}

impl CoreInfo {
    pub fn new() -> Self {
        Self {
            freq_mhz: 0,
            util_pct: 0.0,
            prev_idle: 0,
            prev_total: 0,
        }
    }
}

// ── Workload auto-detection ──────────────────────────────────────────────────

const WORKLOAD_START_UTIL: f64 = 25.0;
const WORKLOAD_END_UTIL: f64 = 15.0;
const WORKLOAD_SETTLE_TICKS: u32 = 10;

#[derive(Clone)]
pub struct WorkloadSegment {
    pub id: u32,
    pub start_wall: (u8, u8, u8),
    pub end_wall: (u8, u8, u8),
    pub duration_secs: f64,
    pub start_temp: u32,
    pub peak_temp: u32,
    pub avg_temp: f64,
    pub peak_ppt: f64,
    pub avg_ppt: f64,
    pub energy_wh: f64,
    pub peak_per_core_w: f64,
    pub peak_busy_cores: usize,
    pub avg_freq: u32,
    pub min_freq: u32,
    pub max_freq: u32,
    pub clock_ratio_pct: f64,
    pub avg_util: f64,
    pub peak_util: f64,
    pub avg_efficiency: f64,
    pub thermal_events: u32,
    pub power_events: u32,
    pub throttle_secs: f64,
}

pub struct ActiveWorkload {
    pub id: u32,
    pub start_wall: (u8, u8, u8),
    pub start_instant: Instant,
    pub samples: u32,
    temp_sum: u64,
    pub start_temp: u32,
    pub peak_temp: u32,
    ppt_sum: f64,
    pub peak_ppt: f64,
    energy_joules: f64,
    peak_per_core_w: f64,
    peak_busy_cores: usize,
    freq_sum: u64,
    min_freq: u32,
    max_freq: u32,
    util_sum: f64,
    pub peak_util: f64,
    eff_sum: f64,
    eff_count: u32,
    pub thermal_events: u32,
    pub power_events: u32,
    throttle_ticks: u32,
    cooldown_ticks: u32,
    cooldown_temp_sum: u64,
    cooldown_ppt_sum: f64,
    cooldown_freq_sum: u64,
    cooldown_util_sum: f64,
    cooldown_energy_joules: f64,
    cooldown_eff_sum: f64,
    cooldown_eff_count: u32,
    cooldown_throttle_ticks: u32,
    cooldown_samples: u32,
}

impl ActiveWorkload {
    fn new(id: u32, start_temp: u32) -> Self {
        Self {
            id,
            start_wall: wall_clock_hms(),
            start_instant: Instant::now(),
            samples: 0,
            temp_sum: 0,
            start_temp,
            peak_temp: start_temp,
            ppt_sum: 0.0,
            peak_ppt: 0.0,
            energy_joules: 0.0,
            peak_per_core_w: 0.0,
            peak_busy_cores: 0,
            freq_sum: 0,
            min_freq: u32::MAX,
            max_freq: 0,
            util_sum: 0.0,
            peak_util: 0.0,
            eff_sum: 0.0,
            eff_count: 0,
            thermal_events: 0,
            power_events: 0,
            throttle_ticks: 0,
            cooldown_ticks: 0,
            cooldown_temp_sum: 0,
            cooldown_ppt_sum: 0.0,
            cooldown_freq_sum: 0,
            cooldown_util_sum: 0.0,
            cooldown_energy_joules: 0.0,
            cooldown_eff_sum: 0.0,
            cooldown_eff_count: 0,
            cooldown_throttle_ticks: 0,
            cooldown_samples: 0,
        }
    }

    fn accumulate(
        &mut self,
        temp: u32,
        ppt: f64,
        freq: u32,
        util: f64,
        busy_cores: usize,
        throttle_active: bool,
        interval_secs: f64,
        in_cooldown: bool,
    ) {
        self.samples += 1;
        self.temp_sum += temp as u64;
        if temp > self.peak_temp {
            self.peak_temp = temp;
        }
        self.ppt_sum += ppt;
        if ppt > self.peak_ppt {
            self.peak_ppt = ppt;
        }
        self.energy_joules += ppt * interval_secs;
        if busy_cores > 0 {
            let per_core = ppt / busy_cores as f64;
            if per_core > self.peak_per_core_w {
                self.peak_per_core_w = per_core;
            }
            if busy_cores > self.peak_busy_cores {
                self.peak_busy_cores = busy_cores;
            }
        }
        self.freq_sum += freq as u64;
        if freq < self.min_freq {
            self.min_freq = freq;
        }
        if freq > self.max_freq {
            self.max_freq = freq;
        }
        self.util_sum += util;
        if util > self.peak_util {
            self.peak_util = util;
        }
        if ppt > 0.5 {
            self.eff_sum += freq as f64 / ppt;
            self.eff_count += 1;
        }
        if throttle_active {
            self.throttle_ticks += 1;
        }

        if in_cooldown {
            self.cooldown_samples += 1;
            self.cooldown_temp_sum += temp as u64;
            self.cooldown_ppt_sum += ppt;
            self.cooldown_freq_sum += freq as u64;
            self.cooldown_util_sum += util;
            self.cooldown_energy_joules += ppt * interval_secs;
            if ppt > 0.5 {
                self.cooldown_eff_sum += freq as f64 / ppt;
                self.cooldown_eff_count += 1;
            }
            if throttle_active {
                self.cooldown_throttle_ticks += 1;
            }
        }
    }

    fn reset_cooldown(&mut self) {
        self.cooldown_ticks = 0;
        self.cooldown_samples = 0;
        self.cooldown_temp_sum = 0;
        self.cooldown_ppt_sum = 0.0;
        self.cooldown_freq_sum = 0;
        self.cooldown_util_sum = 0.0;
        self.cooldown_energy_joules = 0.0;
        self.cooldown_eff_sum = 0.0;
        self.cooldown_eff_count = 0;
        self.cooldown_throttle_ticks = 0;
    }

    fn finalize(&self, max_freq_mhz: u32, interval_secs: f64) -> WorkloadSegment {
        let active_samples = self.samples.saturating_sub(self.cooldown_samples).max(1);
        let n = active_samples as f64;
        let temp_sum = self.temp_sum.saturating_sub(self.cooldown_temp_sum);
        let ppt_sum = self.ppt_sum - self.cooldown_ppt_sum;
        let freq_sum = self.freq_sum.saturating_sub(self.cooldown_freq_sum);
        let util_sum = self.util_sum - self.cooldown_util_sum;
        let energy_j = self.energy_joules - self.cooldown_energy_joules;
        let eff_count = self.eff_count.saturating_sub(self.cooldown_eff_count);
        let eff_sum = self.eff_sum - self.cooldown_eff_sum;
        let throttle_ticks = self
            .throttle_ticks
            .saturating_sub(self.cooldown_throttle_ticks);

        let cooldown_secs = self.cooldown_ticks as f64 * interval_secs;
        let duration = self.start_instant.elapsed().as_secs_f64() - cooldown_secs;

        let avg_freq = (freq_sum as f64 / n) as u32;
        WorkloadSegment {
            id: self.id,
            start_wall: self.start_wall,
            end_wall: wall_clock_hms(),
            duration_secs: duration.max(0.0),
            start_temp: self.start_temp,
            peak_temp: self.peak_temp,
            avg_temp: temp_sum as f64 / n,
            peak_ppt: self.peak_ppt,
            avg_ppt: ppt_sum / n,
            energy_wh: energy_j / 3600.0,
            peak_per_core_w: self.peak_per_core_w,
            peak_busy_cores: self.peak_busy_cores,
            avg_freq,
            min_freq: if self.min_freq == u32::MAX {
                0
            } else {
                self.min_freq
            },
            max_freq: self.max_freq,
            clock_ratio_pct: if max_freq_mhz > 0 {
                avg_freq as f64 / max_freq_mhz as f64 * 100.0
            } else {
                0.0
            },
            avg_util: util_sum / n,
            peak_util: self.peak_util,
            avg_efficiency: if eff_count > 0 {
                eff_sum / eff_count as f64
            } else {
                0.0
            },
            thermal_events: self.thermal_events,
            power_events: self.power_events,
            throttle_secs: throttle_ticks as f64 * interval_secs,
        }
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start_instant.elapsed().as_secs_f64()
    }
}

// ── SystemState ──────────────────────────────────────────────────────────────

pub struct SystemState {
    pub cores: Vec<CoreInfo>,
    pub temp_c: u32,
    pub temp_history: VecDeque<u64>,
    pub freq_history: VecDeque<u64>,
    pub max_freq_mhz: u32,
    pub events: Vec<ThermalEvent>,
    pub throttle_active: bool,
    pub throttle_reason: ThrottleReason,
    pub start_time: Instant,
    pub peak_temp: u32,
    pub peak_freq: u32,
    pub thermal_count: u32,
    pub power_count: u32,
    pub prev_throttle: bool,
    pub throttle_changed_at: Option<Instant>,
    pub throttle_total_ticks: u64,
    pub ppt_watts: f64,
    pub ppt_history: VecDeque<u64>,
    pub peak_ppt: f64,
    pub util_history: VecDeque<u64>,
    pub temp_duration_ticks: [u32; 106],
    pub temp_streak_current: [u32; 106],
    pub temp_streak_max: [u32; 106],
    pub sample_interval_secs: f64,
    pub initial_efficiency: Option<f64>,
    pub energy_joules: f64,
    pub fan_rpm: u32,
    pub fan_history: VecDeque<u64>,
    pub peak_fan: u32,
    pub active_workload: Option<ActiveWorkload>,
    pub completed_workloads: Vec<WorkloadSegment>,
    workload_counter: u32,
    pub(crate) n_cores: usize,

    // Linux-specific sensor state
    #[cfg(target_os = "linux")]
    pub(super) hwmon_path: Option<std::path::PathBuf>,
    #[cfg(target_os = "linux")]
    pub(super) rapl_package_path: Option<String>,
    #[cfg(target_os = "linux")]
    pub(super) rapl_prev_energy_uj: u64,
    #[cfg(target_os = "linux")]
    pub(super) rapl_prev_time: Option<std::time::Instant>,
    #[cfg(target_os = "linux")]
    pub(super) fan_path: Option<String>,

    // Windows-specific sensor state
    #[cfg(target_os = "windows")]
    pub(super) lhm_available: bool,
    #[cfg(target_os = "windows")]
    pub(super) native_thermal_available: bool,
    #[cfg(target_os = "windows")]
    pub(super) fan_detected: bool,
    /// Consecutive ticks where LHM CPU load diverges from NT-derived utilization.
    /// When this exceeds the threshold, LHM is considered stale.
    #[cfg(target_os = "windows")]
    pub(super) lhm_stale_ticks: u32,
}

impl SystemState {
    pub fn new(n_cores: usize, max_freq_mhz: u32) -> Self {
        let mut cores = Vec::with_capacity(n_cores);
        for _ in 0..n_cores {
            cores.push(CoreInfo::new());
        }

        let pf = platform::init_platform();

        Self {
            cores,
            temp_c: 0,
            temp_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            freq_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            max_freq_mhz,
            events: Vec::new(),
            throttle_active: false,
            throttle_reason: ThrottleReason::None,
            start_time: Instant::now(),
            peak_temp: 0,
            peak_freq: 0,
            thermal_count: 0,
            power_count: 0,
            prev_throttle: false,
            throttle_changed_at: None,
            throttle_total_ticks: 0,
            ppt_watts: 0.0,
            ppt_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            peak_ppt: 0.0,
            util_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            temp_duration_ticks: [0u32; 106],
            temp_streak_current: [0u32; 106],
            temp_streak_max: [0u32; 106],
            sample_interval_secs: 0.5,
            initial_efficiency: None,
            energy_joules: 0.0,
            fan_rpm: 0,
            fan_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            peak_fan: 0,
            active_workload: None,
            completed_workloads: Vec::new(),
            workload_counter: 0,
            n_cores,

            #[cfg(target_os = "linux")]
            hwmon_path: pf.hwmon_path,
            #[cfg(target_os = "linux")]
            rapl_package_path: pf.rapl_package_path,
            #[cfg(target_os = "linux")]
            rapl_prev_energy_uj: 0,
            #[cfg(target_os = "linux")]
            rapl_prev_time: None,
            #[cfg(target_os = "linux")]
            fan_path: pf.fan_path,

            #[cfg(target_os = "windows")]
            lhm_available: pf.lhm_available,
            #[cfg(target_os = "windows")]
            native_thermal_available: pf.native_thermal_available,
            #[cfg(target_os = "windows")]
            fan_detected: false,
            #[cfg(target_os = "windows")]
            lhm_stale_ticks: 0,
        }
    }

    /// Read all sensors and update history, peaks, throttle detection, workloads.
    pub fn sample(&mut self) {
        // Platform-specific sensor reading (defined in linux.rs / windows.rs)
        self.sample_sensors();

        // ── History ──────────────────────────────────────────────────────
        self.temp_history.push_back(self.temp_c as u64);
        if self.temp_history.len() > HISTORY_LEN {
            self.temp_history.pop_front();
        }

        // Track time spent at each temperature
        let idx = (self.temp_c as usize).min(105);
        self.temp_duration_ticks[idx] = self.temp_duration_ticks[idx].saturating_add(1);

        // Track continuous streaks at or above each temperature
        for t in 0..=105 {
            if self.temp_c as usize >= t {
                self.temp_streak_current[t] = self.temp_streak_current[t].saturating_add(1);
                if self.temp_streak_current[t] > self.temp_streak_max[t] {
                    self.temp_streak_max[t] = self.temp_streak_current[t];
                }
            } else {
                self.temp_streak_current[t] = 0;
            }
        }

        let avg_freq = self.avg_freq();
        self.freq_history.push_back(avg_freq as u64);
        if self.freq_history.len() > HISTORY_LEN {
            self.freq_history.pop_front();
        }

        self.ppt_history
            .push_back((self.ppt_watts * 10.0) as u64);
        if self.ppt_history.len() > HISTORY_LEN {
            self.ppt_history.pop_front();
        }

        let avg_util = self.avg_util();
        self.util_history.push_back((avg_util * 10.0) as u64);
        if self.util_history.len() > HISTORY_LEN {
            self.util_history.pop_front();
        }

        self.fan_history.push_back(self.fan_rpm as u64);
        if self.fan_history.len() > HISTORY_LEN {
            self.fan_history.pop_front();
        }

        // ── Peaks ────────────────────────────────────────────────────────
        if self.temp_c > self.peak_temp {
            self.peak_temp = self.temp_c;
        }
        let max_f = self.cores.iter().map(|c| c.freq_mhz).max().unwrap_or(0);
        if max_f > self.peak_freq {
            self.peak_freq = max_f;
        }
        if self.ppt_watts > self.peak_ppt {
            self.peak_ppt = self.ppt_watts;
        }
        if self.fan_rpm > self.peak_fan {
            self.peak_fan = self.fan_rpm;
        }

        // Record initial efficiency baseline
        if self.initial_efficiency.is_none() && self.ppt_watts > 5.0 {
            self.initial_efficiency = Some(self.avg_freq() as f64 / self.ppt_watts);
        }

        // ── Throttle detection ───────────────────────────────────────────
        self.analyze_throttle();
        if self.throttle_active {
            self.throttle_total_ticks += 1;
        }

        // Event logging (rising edge only)
        let throttle_rising_edge = self.throttle_active && !self.prev_throttle;
        if throttle_rising_edge {
            match self.throttle_reason {
                ThrottleReason::Thermal => self.thermal_count += 1,
                ThrottleReason::Power => self.power_count += 1,
                ThrottleReason::None => {}
            }
            self.events.push(ThermalEvent {
                wall_time: wall_clock_hms(),
                temp_c: self.temp_c,
                ppt_watts: self.ppt_watts,
                reason: self.throttle_reason,
            });
            if self.events.len() > 50 {
                self.events.remove(0);
            }
        }
        if self.throttle_active != self.prev_throttle {
            self.throttle_changed_at = Some(Instant::now());
        }
        self.prev_throttle = self.throttle_active;

        // Energy tracking (cumulative joules)
        self.energy_joules += self.ppt_watts * self.sample_interval_secs;

        // ── Workload auto-detection ──────────────────────────────────────
        let avg_util_now = self.avg_util();
        let busy_cores = self.busy_core_count();
        if let Some(ref mut wl) = self.active_workload {
            let in_cooldown = wl.cooldown_ticks > 0;
            wl.accumulate(
                self.temp_c,
                self.ppt_watts,
                avg_freq,
                avg_util_now,
                busy_cores,
                self.throttle_active,
                self.sample_interval_secs,
                in_cooldown || avg_util_now < WORKLOAD_END_UTIL,
            );
            if throttle_rising_edge {
                match self.throttle_reason {
                    ThrottleReason::Thermal => wl.thermal_events += 1,
                    ThrottleReason::Power => wl.power_events += 1,
                    ThrottleReason::None => {}
                }
            }
            if avg_util_now < WORKLOAD_END_UTIL {
                wl.cooldown_ticks += 1;
                if wl.cooldown_ticks >= WORKLOAD_SETTLE_TICKS {
                    let segment = wl.finalize(self.max_freq_mhz, self.sample_interval_secs);
                    self.completed_workloads.push(segment);
                    if self.completed_workloads.len() > 20 {
                        self.completed_workloads.remove(0);
                    }
                    self.active_workload = None;
                }
            } else if wl.cooldown_ticks > 0 {
                wl.reset_cooldown();
            }
        } else if avg_util_now >= WORKLOAD_START_UTIL {
            self.workload_counter += 1;
            self.active_workload = Some(ActiveWorkload::new(self.workload_counter, self.temp_c));
        }
    }

    pub fn temp_rate(&self) -> f64 {
        let n = 6usize.min(self.temp_history.len());
        if n < 2 {
            return 0.0;
        }
        let oldest = *self.temp_history.iter().rev().nth(n - 1).unwrap_or(&0);
        let dt = (n - 1) as f64 * self.sample_interval_secs;
        if dt > 0.0 {
            (self.temp_c as f64 - oldest as f64) / dt
        } else {
            0.0
        }
    }

    pub fn avg_temp(&self) -> f64 {
        if self.temp_history.is_empty() {
            return 0.0;
        }
        self.temp_history.iter().sum::<u64>() as f64 / self.temp_history.len() as f64
    }

    pub fn avg_ppt(&self) -> f64 {
        if self.ppt_history.is_empty() {
            return 0.0;
        }
        self.ppt_history.iter().sum::<u64>() as f64 / self.ppt_history.len() as f64 / 10.0
    }

    pub fn energy_wh(&self) -> f64 {
        self.energy_joules / 3600.0
    }

    pub fn temp_stability(&self) -> f64 {
        let n = 60usize.min(self.temp_history.len());
        if n < 2 {
            return 0.0;
        }
        let vals: Vec<f64> = self
            .temp_history
            .iter()
            .rev()
            .take(n)
            .map(|&v| v as f64)
            .collect();
        let mean = vals.iter().sum::<f64>() / n as f64;
        let variance = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
        variance.sqrt()
    }

    pub fn avg_fan(&self) -> f64 {
        if self.fan_history.is_empty() {
            return 0.0;
        }
        self.fan_history.iter().sum::<u64>() as f64 / self.fan_history.len() as f64
    }

    pub fn busy_core_count(&self) -> usize {
        self.cores.iter().filter(|c| c.util_pct > 20.0).count()
    }

    pub fn throttle_total_secs(&self) -> f64 {
        self.throttle_total_ticks as f64 * self.sample_interval_secs
    }

    pub fn avg_util(&self) -> f64 {
        if self.cores.is_empty() {
            return 0.0;
        }
        self.cores.iter().map(|c| c.util_pct).sum::<f64>() / self.cores.len() as f64
    }

    pub fn avg_freq(&self) -> u32 {
        if self.cores.is_empty() {
            return 0;
        }
        let sum: u64 = self.cores.iter().map(|c| c.freq_mhz as u64).sum();
        (sum / self.cores.len() as u64) as u32
    }

    fn analyze_throttle(&mut self) {
        if self.cores.is_empty() || self.max_freq_mhz == 0 {
            self.throttle_active = false;
            self.throttle_reason = ThrottleReason::None;
            return;
        }

        // If no frequency data is available (all cores at 0), skip detection
        if self.cores.iter().all(|c| c.freq_mhz == 0) {
            self.throttle_active = false;
            self.throttle_reason = ThrottleReason::None;
            return;
        }

        let freq_threshold = (self.max_freq_mhz as f64 * THROTTLE_FREQ_RATIO) as u32;
        let mut busy_cores = 0u32;
        let mut busy_slow_cores = 0u32;

        for c in &self.cores {
            if c.util_pct > THROTTLE_UTIL_THRESH {
                busy_cores += 1;
                if c.freq_mhz < freq_threshold {
                    busy_slow_cores += 1;
                }
            }
        }

        if busy_cores == 0 {
            self.throttle_active = false;
            self.throttle_reason = ThrottleReason::None;
            return;
        }

        let slow_ratio = busy_slow_cores as f64 / busy_cores as f64;
        if slow_ratio < 0.3 {
            self.throttle_active = false;
            self.throttle_reason = ThrottleReason::None;
            return;
        }

        self.throttle_active = true;
        self.throttle_reason = if self.temp_c >= THERMAL_TEMP_THRESH {
            ThrottleReason::Thermal
        } else {
            ThrottleReason::Power
        };
    }
}

pub fn get_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a SystemState with synthetic values for testing.
    /// Does not probe hardware — all sensor paths are None.
    fn test_state(n_cores: usize, max_freq_mhz: u32) -> SystemState {
        let mut cores = Vec::with_capacity(n_cores);
        for _ in 0..n_cores {
            cores.push(CoreInfo::new());
        }
        SystemState {
            cores,
            temp_c: 0,
            temp_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            freq_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            max_freq_mhz,
            events: Vec::new(),
            throttle_active: false,
            throttle_reason: ThrottleReason::None,
            start_time: Instant::now(),
            peak_temp: 0,
            peak_freq: 0,
            thermal_count: 0,
            power_count: 0,
            prev_throttle: false,
            throttle_changed_at: None,
            throttle_total_ticks: 0,
            ppt_watts: 0.0,
            ppt_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            peak_ppt: 0.0,
            util_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            temp_duration_ticks: [0u32; 106],
            temp_streak_current: [0u32; 106],
            temp_streak_max: [0u32; 106],
            sample_interval_secs: 0.5,
            initial_efficiency: None,
            energy_joules: 0.0,
            fan_rpm: 0,
            fan_history: VecDeque::with_capacity(HISTORY_LEN + 1),
            peak_fan: 0,
            active_workload: None,
            completed_workloads: Vec::new(),
            workload_counter: 0,
            n_cores,

            #[cfg(target_os = "linux")]
            hwmon_path: None,
            #[cfg(target_os = "linux")]
            rapl_package_path: None,
            #[cfg(target_os = "linux")]
            rapl_prev_energy_uj: 0,
            #[cfg(target_os = "linux")]
            rapl_prev_time: None,
            #[cfg(target_os = "linux")]
            fan_path: None,

            #[cfg(target_os = "windows")]
            lhm_available: false,
            #[cfg(target_os = "windows")]
            native_thermal_available: false,
            #[cfg(target_os = "windows")]
            fan_detected: false,
            #[cfg(target_os = "windows")]
            lhm_stale_ticks: 0,
        }
    }

    // ── avg_freq / avg_util ──────────────────────────────────────────────

    #[test]
    fn avg_freq_with_known_values() {
        let mut s = test_state(4, 5000);
        s.cores[0].freq_mhz = 4000;
        s.cores[1].freq_mhz = 4200;
        s.cores[2].freq_mhz = 4400;
        s.cores[3].freq_mhz = 4800;
        assert_eq!(s.avg_freq(), 4350);
    }

    #[test]
    fn avg_freq_empty_cores() {
        let s = test_state(0, 5000);
        assert_eq!(s.avg_freq(), 0);
    }

    #[test]
    fn avg_util_with_known_values() {
        let mut s = test_state(4, 5000);
        s.cores[0].util_pct = 0.0;
        s.cores[1].util_pct = 50.0;
        s.cores[2].util_pct = 100.0;
        s.cores[3].util_pct = 50.0;
        let avg = s.avg_util();
        assert!((avg - 50.0).abs() < 0.01);
    }

    #[test]
    fn avg_util_empty_cores() {
        let s = test_state(0, 5000);
        assert!((s.avg_util() - 0.0).abs() < f64::EPSILON);
    }

    // ── temp_rate ────────────────────────────────────────────────────────

    #[test]
    fn temp_rate_rising() {
        let mut s = test_state(4, 5000);
        s.sample_interval_secs = 0.5;
        for t in &[60u64, 62, 64, 66, 68, 70] {
            s.temp_history.push_back(*t);
        }
        s.temp_c = 70;
        let rate = s.temp_rate();
        assert!((rate - 4.0).abs() < 0.01);
    }

    #[test]
    fn temp_rate_stable() {
        let mut s = test_state(4, 5000);
        for _ in 0..6 {
            s.temp_history.push_back(65);
        }
        s.temp_c = 65;
        assert!((s.temp_rate() - 0.0).abs() < 0.01);
    }

    #[test]
    fn temp_rate_insufficient_history() {
        let mut s = test_state(4, 5000);
        s.temp_history.push_back(65);
        s.temp_c = 65;
        assert!((s.temp_rate() - 0.0).abs() < f64::EPSILON);
    }

    // ── temp_stability ───────────────────────────────────────────────────

    #[test]
    fn temp_stability_constant() {
        let mut s = test_state(4, 5000);
        for _ in 0..60 {
            s.temp_history.push_back(70);
        }
        assert!((s.temp_stability() - 0.0).abs() < 0.01);
    }

    #[test]
    fn temp_stability_varying() {
        let mut s = test_state(4, 5000);
        for i in 0..60 {
            s.temp_history.push_back(if i % 2 == 0 { 60 } else { 80 });
        }
        assert!((s.temp_stability() - 10.0).abs() < 0.01);
    }

    // ── energy_wh ────────────────────────────────────────────────────────

    #[test]
    fn energy_wh_conversion() {
        let mut s = test_state(4, 5000);
        s.energy_joules = 3600.0;
        assert!((s.energy_wh() - 1.0).abs() < f64::EPSILON);
    }

    // ── busy_core_count ──────────────────────────────────────────────────

    #[test]
    fn busy_core_count_threshold() {
        let mut s = test_state(4, 5000);
        s.cores[0].util_pct = 5.0;
        s.cores[1].util_pct = 20.0;
        s.cores[2].util_pct = 21.0;
        s.cores[3].util_pct = 90.0;
        assert_eq!(s.busy_core_count(), 2);
    }

    // ── analyze_throttle ─────────────────────────────────────────────────

    #[test]
    fn no_throttle_when_idle() {
        let mut s = test_state(4, 5000);
        for c in &mut s.cores {
            c.util_pct = 5.0;
            c.freq_mhz = 2000;
        }
        s.analyze_throttle();
        assert!(!s.throttle_active);
        assert_eq!(s.throttle_reason, ThrottleReason::None);
    }

    #[test]
    fn no_throttle_when_busy_at_full_speed() {
        let mut s = test_state(4, 5000);
        for c in &mut s.cores {
            c.util_pct = 80.0;
            c.freq_mhz = 4500;
        }
        s.analyze_throttle();
        assert!(!s.throttle_active);
    }

    #[test]
    fn thermal_throttle_detected() {
        let mut s = test_state(4, 5000);
        s.temp_c = 90;
        for c in &mut s.cores {
            c.util_pct = 80.0;
            c.freq_mhz = 2000;
        }
        s.analyze_throttle();
        assert!(s.throttle_active);
        assert_eq!(s.throttle_reason, ThrottleReason::Thermal);
    }

    #[test]
    fn power_throttle_detected() {
        let mut s = test_state(4, 5000);
        s.temp_c = 70;
        for c in &mut s.cores {
            c.util_pct = 80.0;
            c.freq_mhz = 2000;
        }
        s.analyze_throttle();
        assert!(s.throttle_active);
        assert_eq!(s.throttle_reason, ThrottleReason::Power);
    }

    #[test]
    fn no_throttle_when_only_some_cores_slow() {
        let mut s = test_state(8, 5000);
        for c in &mut s.cores[..6] {
            c.util_pct = 80.0;
            c.freq_mhz = 4500;
        }
        for c in &mut s.cores[6..] {
            c.util_pct = 80.0;
            c.freq_mhz = 2000;
        }
        s.analyze_throttle();
        assert!(!s.throttle_active);
    }

    // ── has_fan_sensor ───────────────────────────────────────────────────

    #[test]
    fn has_fan_sensor_false_by_default() {
        let s = test_state(4, 5000);
        assert!(!s.has_fan_sensor());
    }

    // ── throttle_total_secs ──────────────────────────────────────────────

    #[test]
    fn throttle_total_secs_conversion() {
        let mut s = test_state(4, 5000);
        s.throttle_total_ticks = 20;
        s.sample_interval_secs = 0.5;
        assert!((s.throttle_total_secs() - 10.0).abs() < f64::EPSILON);
    }

    // ── ThrottleReason::as_str ───────────────────────────────────────────

    #[test]
    fn throttle_reason_strings() {
        assert_eq!(ThrottleReason::None.as_str(), "none");
        assert_eq!(ThrottleReason::Thermal.as_str(), "thermal");
        assert_eq!(ThrottleReason::Power.as_str(), "power");
    }

    // ── avg_ppt with history ─────────────────────────────────────────────

    #[test]
    fn avg_ppt_from_history() {
        let mut s = test_state(4, 5000);
        s.ppt_history.push_back(500);
        s.ppt_history.push_back(600);
        s.ppt_history.push_back(700);
        let avg = s.avg_ppt();
        assert!((avg - 60.0).abs() < 0.01);
    }

    #[test]
    fn avg_ppt_empty() {
        let s = test_state(4, 5000);
        assert!((s.avg_ppt() - 0.0).abs() < f64::EPSILON);
    }

    // ── avg_temp with history ────────────────────────────────────────────

    #[test]
    fn avg_temp_from_history() {
        let mut s = test_state(4, 5000);
        s.temp_history.push_back(60);
        s.temp_history.push_back(70);
        s.temp_history.push_back(80);
        let avg = s.avg_temp();
        assert!((avg - 70.0).abs() < 0.01);
    }

    // ── avg_fan with history ─────────────────────────────────────────────

    #[test]
    fn avg_fan_from_history() {
        let mut s = test_state(4, 5000);
        s.fan_history.push_back(1000);
        s.fan_history.push_back(1200);
        s.fan_history.push_back(1400);
        let avg = s.avg_fan();
        assert!((avg - 1200.0).abs() < 0.01);
    }
}
