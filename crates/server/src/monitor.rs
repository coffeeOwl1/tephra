use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    prev_idle: u64,
    prev_total: u64,
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
    fan_path: Option<String>,
    pub active_workload: Option<ActiveWorkload>,
    pub completed_workloads: Vec<WorkloadSegment>,
    workload_counter: u32,
    rapl_package_path: Option<String>,
    rapl_prev_energy_uj: u64,
    rapl_prev_time: Option<std::time::Instant>,
    hwmon_path: Option<PathBuf>,
    n_cores: usize,
}

impl SystemState {
    pub fn new(n_cores: usize, max_freq_mhz: u32) -> Self {
        let hwmon_path = find_hwmon("k10temp")
            .or_else(|| find_hwmon("coretemp"))
            .or_else(|| find_hwmon("zenpower"));

        let rapl_package_path = find_rapl_package();

        let mut cores = Vec::with_capacity(n_cores);
        for _ in 0..n_cores {
            cores.push(CoreInfo::new());
        }

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
            fan_path: find_fan_sensor(),
            active_workload: None,
            completed_workloads: Vec::new(),
            workload_counter: 0,
            rapl_package_path,
            rapl_prev_energy_uj: 0,
            rapl_prev_time: None,
            hwmon_path,
            n_cores,
        }
    }

    pub fn sample(&mut self) {
        self.sample_frequencies();
        self.sample_utilization();
        self.sample_temperature();
        self.sample_ppt();
        self.sample_fan();

        // History
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
        self.util_history
            .push_back((avg_util * 10.0) as u64);
        if self.util_history.len() > HISTORY_LEN {
            self.util_history.pop_front();
        }

        self.fan_history.push_back(self.fan_rpm as u64);
        if self.fan_history.len() > HISTORY_LEN {
            self.fan_history.pop_front();
        }

        // Peaks
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

        // Throttle detection
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

        // Workload auto-detection
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

    pub fn has_fan_sensor(&self) -> bool {
        self.fan_path.is_some()
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

    fn sample_frequencies(&mut self) {
        for i in 0..self.n_cores {
            let path = format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq",
                i
            );
            let khz = read_int(&path);
            self.cores[i].freq_mhz = (khz / 1000) as u32;
        }
    }

    fn sample_utilization(&mut self) {
        let content = match fs::read_to_string("/proc/stat") {
            Ok(c) => c,
            Err(_) => return,
        };

        for line in content.lines() {
            if !line.starts_with("cpu") || line.starts_with("cpu ") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 8 {
                continue;
            }
            let idx: usize = match parts[0].strip_prefix("cpu").and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => continue,
            };
            if idx >= self.n_cores {
                continue;
            }

            let values: Vec<u64> = parts[1..8].iter().filter_map(|s| s.parse().ok()).collect();
            if values.len() < 7 {
                continue;
            }

            let idle = values[3] + values[4];
            let total: u64 = values.iter().sum();

            let core = &mut self.cores[idx];
            if core.prev_total > 0 {
                let d_total = total.saturating_sub(core.prev_total);
                let d_idle = idle.saturating_sub(core.prev_idle);
                if d_total > 0 {
                    core.util_pct = 100.0 * (1.0 - d_idle as f64 / d_total as f64);
                } else {
                    core.util_pct = 0.0;
                }
            }
            core.prev_idle = idle;
            core.prev_total = total;
        }
    }

    fn sample_temperature(&mut self) {
        if let Some(ref path) = self.hwmon_path {
            let temp_path = path.join("temp1_input");
            let raw = read_int(&temp_path.to_string_lossy());
            if raw > 0 {
                self.temp_c = (raw / 1000) as u32;
                return;
            }
        }
        // Fallback: thermal_zone
        for i in 0..10 {
            let path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
            let raw = read_int(&path);
            if raw > 0 {
                self.temp_c = (raw / 1000) as u32;
                return;
            }
        }
    }

    fn sample_ppt(&mut self) {
        if let Some(ref path) = self.rapl_package_path {
            let energy_uj = read_int(path) as u64;
            let now = std::time::Instant::now();

            if let Some(prev_time) = self.rapl_prev_time {
                let dt = now.duration_since(prev_time).as_secs_f64();
                if dt > 0.0 && energy_uj >= self.rapl_prev_energy_uj {
                    let d_energy = energy_uj - self.rapl_prev_energy_uj;
                    self.ppt_watts = d_energy as f64 / (dt * 1_000_000.0);
                }
                // Handle RAPL counter overflow
                if energy_uj < self.rapl_prev_energy_uj {
                    self.rapl_prev_energy_uj = energy_uj;
                    self.rapl_prev_time = Some(now);
                    return;
                }
            }

            self.rapl_prev_energy_uj = energy_uj;
            self.rapl_prev_time = Some(now);
        } else {
            self.ppt_watts = 0.0;
        }
    }

    fn sample_fan(&mut self) {
        if let Some(ref path) = self.fan_path {
            let rpm = read_int(path);
            if rpm >= 0 {
                self.fan_rpm = rpm as u32;
            }
        }
    }

    fn analyze_throttle(&mut self) {
        if self.cores.is_empty() || self.max_freq_mhz == 0 {
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

// ── Hardware detection ───────────────────────────────────────────────────────

fn find_hwmon(name: &str) -> Option<PathBuf> {
    let base = Path::new("/sys/class/hwmon");
    if !base.exists() {
        return None;
    }
    for entry in fs::read_dir(base).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let name_file = path.join("name");
        if let Ok(content) = fs::read_to_string(&name_file) {
            if content.trim() == name {
                return Some(path);
            }
        }
    }
    None
}

fn find_rapl_package() -> Option<String> {
    let path = "/sys/class/powercap/intel-rapl:0/energy_uj";
    if Path::new(path).exists() {
        if fs::read_to_string(path).is_ok() {
            return Some(path.to_string());
        }
    }
    for i in 0..4 {
        let p = format!("/sys/class/powercap/intel-rapl:{}/energy_uj", i);
        if Path::new(&p).exists() {
            if let Ok(content) = fs::read_to_string(&p) {
                if content.trim().parse::<u64>().is_ok() {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn find_fan_sensor() -> Option<String> {
    let base = Path::new("/sys/class/hwmon");
    if !base.exists() {
        return None;
    }
    for entry in fs::read_dir(base).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        for i in 1..=4 {
            let fan_path = path.join(format!("fan{}_input", i));
            if fan_path.exists() {
                if let Ok(content) = fs::read_to_string(&fan_path) {
                    if content.trim().parse::<u32>().is_ok() {
                        return Some(fan_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    None
}

fn wall_clock_hms() -> (u8, u8, u8) {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    unsafe {
        libc::gettimeofday(&mut tv, std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&tv.tv_sec, &mut tm);
        (tm.tm_hour as u8, tm.tm_min as u8, tm.tm_sec as u8)
    }
}

fn read_int(path: &str) -> i64 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn get_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

pub fn get_cpu_model() -> String {
    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        for line in content.lines() {
            if line.starts_with("model name") {
                if let Some(val) = line.split(':').nth(1) {
                    return val.trim().to_string();
                }
            }
        }
    }
    "Unknown CPU".to_string()
}

pub fn get_max_freq() -> u32 {
    let khz = read_int("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq");
    if khz > 0 {
        (khz / 1000) as u32
    } else {
        5000
    }
}

pub fn get_scaling_driver() -> String {
    fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn get_governor() -> String {
    fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn get_total_ram_gb() -> f64 {
    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<f64>() {
                        return (kb / 1_048_576.0 * 10.0).round() / 10.0;
                    }
                }
            }
        }
    }
    0.0
}

pub fn get_hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
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
            fan_path: None,
            active_workload: None,
            completed_workloads: Vec::new(),
            workload_counter: 0,
            rapl_package_path: None,
            rapl_prev_energy_uj: 0,
            rapl_prev_time: None,
            hwmon_path: None,
            n_cores,
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
        // Push 6 samples: 60, 62, 64, 66, 68, 70
        for t in &[60u64, 62, 64, 66, 68, 70] {
            s.temp_history.push_back(*t);
        }
        s.temp_c = 70;
        let rate = s.temp_rate();
        // Over 5 intervals of 0.5s = 2.5s, temp rose 10°C → 4°C/s
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
        // Alternating 60/80 → mean=70, each deviation=10, stddev=10
        assert!((s.temp_stability() - 10.0).abs() < 0.01);
    }

    // ── energy_wh ────────────────────────────────────────────────────────

    #[test]
    fn energy_wh_conversion() {
        let mut s = test_state(4, 5000);
        s.energy_joules = 3600.0; // 1 Wh
        assert!((s.energy_wh() - 1.0).abs() < f64::EPSILON);
    }

    // ── busy_core_count ──────────────────────────────────────────────────

    #[test]
    fn busy_core_count_threshold() {
        let mut s = test_state(4, 5000);
        s.cores[0].util_pct = 5.0; // not busy
        s.cores[1].util_pct = 20.0; // not busy (threshold is >20)
        s.cores[2].util_pct = 21.0; // busy
        s.cores[3].util_pct = 90.0; // busy
        assert_eq!(s.busy_core_count(), 2);
    }

    // ── analyze_throttle ─────────────────────────────────────────────────

    #[test]
    fn no_throttle_when_idle() {
        let mut s = test_state(4, 5000);
        // All cores idle
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
            c.freq_mhz = 4500; // well above 60% of 5000 = 3000
        }
        s.analyze_throttle();
        assert!(!s.throttle_active);
    }

    #[test]
    fn thermal_throttle_detected() {
        let mut s = test_state(4, 5000);
        s.temp_c = 90; // above THERMAL_TEMP_THRESH (85)
        for c in &mut s.cores {
            c.util_pct = 80.0;
            c.freq_mhz = 2000; // below 60% of 5000 = 3000
        }
        s.analyze_throttle();
        assert!(s.throttle_active);
        assert_eq!(s.throttle_reason, ThrottleReason::Thermal);
    }

    #[test]
    fn power_throttle_detected() {
        let mut s = test_state(4, 5000);
        s.temp_c = 70; // below thermal threshold
        for c in &mut s.cores {
            c.util_pct = 80.0;
            c.freq_mhz = 2000; // below 60% of 5000
        }
        s.analyze_throttle();
        assert!(s.throttle_active);
        assert_eq!(s.throttle_reason, ThrottleReason::Power);
    }

    #[test]
    fn no_throttle_when_only_some_cores_slow() {
        let mut s = test_state(8, 5000);
        // 6 cores busy at full speed, 2 slow — ratio 2/8 = 0.25 < 0.3
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
        // ppt_history stores tenths of watts
        s.ppt_history.push_back(500); // 50.0W
        s.ppt_history.push_back(600); // 60.0W
        s.ppt_history.push_back(700); // 70.0W
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
