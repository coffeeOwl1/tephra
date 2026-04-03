use std::fs;
use std::path::{Path, PathBuf};

use super::SystemState;

// ── Platform initialization ──────────────────────────────────────────────────

pub(super) struct PlatformFields {
    pub hwmon_path: Option<PathBuf>,
    pub rapl_package_path: Option<String>,
    pub fan_path: Option<String>,
}

pub(super) fn init_platform() -> PlatformFields {
    PlatformFields {
        hwmon_path: find_hwmon("k10temp")
            .or_else(|| find_hwmon("coretemp"))
            .or_else(|| find_hwmon("zenpower")),
        rapl_package_path: find_rapl_package(),
        fan_path: find_fan_sensor(),
    }
}

// ── Sensor sampling ──────────────────────────────────────────────────────────

impl SystemState {
    pub(super) fn sample_sensors(&mut self) {
        self.sample_frequencies();
        self.sample_utilization();
        self.sample_temperature();
        self.sample_ppt();
        self.sample_fan();
        self.sample_top_processes();
    }

    pub(crate) fn has_fan_sensor(&self) -> bool {
        self.fan_path.is_some()
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

    fn sample_top_processes(&mut self) {
        use std::collections::HashMap;
        use super::{TOP_PROCESS_COUNT, PROC_AVG_WINDOW};

        // Read aggregate CPU total from /proc/stat "cpu " line
        let stat_content = match fs::read_to_string("/proc/stat") {
            Ok(c) => c,
            Err(_) => return,
        };
        let sys_total: u64 = stat_content
            .lines()
            .find(|l| l.starts_with("cpu "))
            .and_then(|line| {
                let vals: u64 = line.split_whitespace().skip(1).take(7)
                    .filter_map(|s| s.parse::<u64>().ok())
                    .sum();
                Some(vals)
            })
            .unwrap_or(0);

        // Scan /proc for process CPU ticks
        let mut current_ticks: HashMap<u32, u64> = HashMap::new();
        let proc_dir = match fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return,
        };

        for entry in proc_dir.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let pid: u32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let stat_path = format!("/proc/{pid}/stat");
            let content = match fs::read_to_string(&stat_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Parse /proc/[pid]/stat — comm field is in parens and may contain spaces
            let comm_start = match content.find('(') {
                Some(i) => i,
                None => continue,
            };
            let comm_end = match content.rfind(')') {
                Some(i) => i,
                None => continue,
            };
            let comm = content[comm_start + 1..comm_end].to_string();
            let rest = &content[comm_end + 2..]; // skip ") "
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.len() < 13 {
                continue;
            }
            let utime: u64 = fields[11].parse().unwrap_or(0);
            let stime: u64 = fields[12].parse().unwrap_or(0);
            current_ticks.insert(pid, utime + stime);
            self.proc_names.insert(pid, comm);
        }

        // Push current snapshot into rolling window
        self.proc_tick_history.push_back((sys_total, current_ticks));
        if self.proc_tick_history.len() > PROC_AVG_WINDOW {
            self.proc_tick_history.pop_front();
        }

        // Need at least 2 snapshots to compute deltas
        if self.proc_tick_history.len() < 2 {
            return;
        }

        // Compare oldest and newest snapshots in the window
        let (old_sys, old_ticks) = self.proc_tick_history.front().unwrap();
        let (new_sys, new_ticks) = self.proc_tick_history.back().unwrap();
        let d_sys = new_sys.saturating_sub(*old_sys);
        if d_sys == 0 {
            return;
        }

        let mut procs: Vec<super::TopProcess> = new_ticks
            .iter()
            .filter_map(|(&pid, &new_t)| {
                let old_t = old_ticks.get(&pid)?;
                let d_proc = new_t.saturating_sub(*old_t);
                if d_proc == 0 {
                    return None;
                }
                let cpu_pct = 100.0 * d_proc as f64 / d_sys as f64;
                let name = self.proc_names.get(&pid)?.clone();
                Some(super::TopProcess { pid, name, cpu_pct })
            })
            .collect();

        procs.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
        procs.truncate(TOP_PROCESS_COUNT);
        self.top_processes = procs;

        // Prune dead processes from names map periodically
        if let Some((_, latest)) = self.proc_tick_history.back() {
            self.proc_names.retain(|pid, _| latest.contains_key(pid));
        }
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

fn read_int(path: &str) -> i64 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

// ── Wall clock ───────────────────────────────────────────────────────────────

pub fn wall_clock_hms() -> (u8, u8, u8) {
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

// ── System info queries ──────────────────────────────────────────────────────

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
