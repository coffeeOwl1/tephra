use std::ffi::c_void;
use std::io::Read as _;

use serde::Deserialize;
use tracing::warn;

use super::SystemState;

const LHM_URL: &str = "http://localhost:8085/data.json";

// ── Platform initialization ──────────────────────────────────────────────────

pub(super) struct PlatformFields {
    pub lhm_available: bool,
}

pub(super) fn init_platform() -> PlatformFields {
    let lhm_available = check_lhm_http();
    if lhm_available {
        tracing::info!("Using LibreHardwareMonitor HTTP server for sensor data");
    } else {
        warn!(
            "LibreHardwareMonitor HTTP server not found at {LHM_URL} — only CPU utilization \
             will be available. Install LibreHardwareMonitor and enable Options > HTTP Server \
             for temperature, power, frequency, and fan data."
        );
    }

    PlatformFields {
        lhm_available,
    }
}

fn check_lhm_http() -> bool {
    http_get_json::<LhmNode>(LHM_URL).is_some()
}

// ── LHM HTTP JSON types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LhmNode {
    #[serde(rename = "Text")]
    text: String,
    #[serde(rename = "SensorId", default)]
    sensor_id: Option<String>,
    #[serde(rename = "Type", default)]
    sensor_type: Option<String>,
    #[serde(rename = "Value", default)]
    value: Option<String>,
    #[serde(rename = "Children", default)]
    children: Vec<LhmNode>,
}

/// Parse the numeric prefix from a value string like "70.0 °C" or "1455 RPM".
fn parse_value(s: &str) -> f64 {
    // Take chars until we hit something that's not a digit, dot, or minus
    let num_str: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    num_str.parse().unwrap_or(0.0)
}

/// Batched sensor data from a single LHM HTTP query.
struct LhmData {
    temp_c: Option<u32>,
    fan_rpm: Option<u32>,
    ppt_watts: Option<f64>,
    per_core_freq: Vec<u32>,
    /// Hash of the raw JSON response body, used for staleness detection.
    /// If LHM's sensor polling has stopped, the HTTP endpoint returns
    /// byte-identical JSON on every request.
    response_hash: u64,
}

fn query_lhm_http() -> Option<LhmData> {
    let (root, response_hash): (LhmNode, u64) = http_get_json_with_hash(LHM_URL)?;

    let mut data = LhmData {
        temp_c: None,
        fan_rpm: None,
        ppt_watts: None,
        per_core_freq: Vec::new(),
        response_hash,
    };

    // Walk the tree collecting sensors
    fn collect(node: &LhmNode, data: &mut LhmData) {
        if let (Some(ref stype), Some(ref val_str), Some(ref sid)) =
            (&node.sensor_type, &node.value, &node.sensor_id)
        {
            let id_lower = sid.to_lowercase();
            let is_cpu = id_lower.contains("/cpu")
                || id_lower.contains("/amdcpu")
                || id_lower.contains("/intelcpu");

            match stype.as_str() {
                "Temperature" => {
                    if is_cpu && data.temp_c.is_none() {
                        let name_lower = node.text.to_lowercase();
                        if name_lower.contains("package")
                            || name_lower.contains("tctl")
                            || name_lower == "core max"
                        {
                            let v = parse_value(val_str);
                            if v > 0.0 {
                                data.temp_c = Some(v.round() as u32);
                            }
                        }
                    }
                }
                "Power" => {
                    if is_cpu && data.ppt_watts.is_none() {
                        let name_lower = node.text.to_lowercase();
                        if name_lower.contains("package") {
                            let v = parse_value(val_str);
                            if v >= 0.0 {
                                data.ppt_watts = Some(v);
                            }
                        }
                    }
                }
                "Fan" => {
                    if data.fan_rpm.is_none() {
                        let v = parse_value(val_str);
                        if v > 0.0 {
                            data.fan_rpm = Some(v.round() as u32);
                        }
                    }
                }
                "Clock" => {
                    if is_cpu && id_lower.contains("/clock/") {
                        let name_lower = node.text.to_lowercase();
                        if !name_lower.contains("bus") {
                            let v = parse_value(val_str);
                            if v > 0.0 {
                                data.per_core_freq.push(v.round() as u32);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        for child in &node.children {
            collect(child, data);
        }
    }

    collect(&root, &mut data);

    // If we didn't find a package/tctl temp, fall back to highest CPU temp
    if data.temp_c.is_none() {
        fn find_max_cpu_temp(node: &LhmNode, max: &mut f64) {
            if let (Some(ref stype), Some(ref val_str), Some(ref sid)) =
                (&node.sensor_type, &node.value, &node.sensor_id)
            {
                let id_lower = sid.to_lowercase();
                if stype == "Temperature"
                    && (id_lower.contains("/cpu")
                        || id_lower.contains("/amdcpu")
                        || id_lower.contains("/intelcpu"))
                    && !node.text.to_lowercase().contains("distance")
                {
                    let v = parse_value(val_str);
                    if v > *max {
                        *max = v;
                    }
                }
            }
            for child in &node.children {
                find_max_cpu_temp(child, max);
            }
        }
        let mut max = 0.0;
        find_max_cpu_temp(&root, &mut max);
        if max > 0.0 {
            data.temp_c = Some(max.round() as u32);
        }
    }

    Some(data)
}

// ── Simple blocking HTTP GET ─────────────────────────────────────────────────

/// FNV-1a hash for fast, non-cryptographic hashing of the response body.
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Like `http_get_json` but also returns a hash of the raw response body,
/// used to detect when LHM serves identical (stale) data across requests.
fn http_get_json_with_hash<T: serde::de::DeserializeOwned>(url: &str) -> Option<(T, u64)> {
    let body = http_get_body(url)?;
    let hash = fnv1a_hash(body.as_bytes());
    let parsed: T = serde_json::from_str(&body).ok()?;
    Some((parsed, hash))
}

fn http_get_body(url: &str) -> Option<String> {
    let url = url.strip_prefix("http://")?;
    let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
    let path = format!("/{path}");

    let stream = std::net::TcpStream::connect(host_port).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .ok()?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_millis(500)))
        .ok()?;

    use std::io::Write;
    let mut stream = stream;
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: {host_port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;

    let response_str = String::from_utf8_lossy(&response);
    let body = response_str.split_once("\r\n\r\n")?.1;
    Some(body.to_string())
}

fn http_get_json<T: serde::de::DeserializeOwned>(url: &str) -> Option<T> {
    let body = http_get_body(url)?;
    serde_json::from_str(&body).ok()
}

// ── NtQuerySystemInformation FFI ─────────────────────────────────────────────

#[repr(C)]
struct ProcessorPerformanceInfo {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: u32 = 8;

// ── CallNtPowerInformation FFI ───────────────────────────────────────────────

#[repr(C)]
struct ProcessorPowerInfo {
    number: u32,
    max_mhz: u32,
    current_mhz: u32,
    mhz_limit: u32,
    max_idle_state: u32,
    current_idle_state: u32,
}

#[link(name = "PowrProf")]
extern "system" {
    fn CallNtPowerInformation(
        information_level: u32,
        input_buffer: *const c_void,
        input_buffer_length: u32,
        output_buffer: *mut c_void,
        output_buffer_length: u32,
    ) -> i32;
}

const PROCESSOR_INFORMATION_LEVEL: u32 = 11;

// ── GlobalMemoryStatusEx FFI ─────────────────────────────────────────────────

#[repr(C)]
struct MemoryStatusEx {
    dw_length: u32,
    dw_memory_load: u32,
    ull_total_phys: u64,
    ull_avail_phys: u64,
    ull_total_page_file: u64,
    ull_avail_page_file: u64,
    ull_total_virtual: u64,
    ull_avail_virtual: u64,
    ull_avail_extended_virtual: u64,
}

extern "system" {
    fn GlobalMemoryStatusEx(lp_buffer: *mut MemoryStatusEx) -> i32;
}

// ── GetLocalTime FFI ─────────────────────────────────────────────────────────

#[repr(C)]
struct WinSystemTime {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

extern "system" {
    fn GetLocalTime(lp_system_time: *mut WinSystemTime);
}

// ── Registry FFI ─────────────────────────────────────────────────────────────

type HKEY = *mut c_void;

extern "system" {
    fn RegOpenKeyExA(
        hkey: HKEY,
        sub_key: *const u8,
        options: u32,
        sam_desired: u32,
        result: *mut HKEY,
    ) -> i32;
    fn RegQueryValueExA(
        hkey: HKEY,
        value_name: *const u8,
        reserved: *const u32,
        value_type: *mut u32,
        data: *mut u8,
        cb_data: *mut u32,
    ) -> i32;
    fn RegCloseKey(hkey: HKEY) -> i32;
}

const HKEY_LOCAL_MACHINE: HKEY = 0x80000002u32 as HKEY;
const KEY_READ: u32 = 0x20019;

fn read_registry_string(subkey: &str, value_name: &str) -> Option<String> {
    let subkey_cstr = format!("{subkey}\0");
    let value_cstr = format!("{value_name}\0");
    let mut hkey: HKEY = std::ptr::null_mut();

    let rc = unsafe {
        RegOpenKeyExA(
            HKEY_LOCAL_MACHINE,
            subkey_cstr.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if rc != 0 {
        return None;
    }

    let mut buf = vec![0u8; 512];
    let mut buf_len = buf.len() as u32;
    let mut value_type: u32 = 0;

    let rc = unsafe {
        RegQueryValueExA(
            hkey,
            value_cstr.as_ptr(),
            std::ptr::null(),
            &mut value_type,
            buf.as_mut_ptr(),
            &mut buf_len,
        )
    };
    unsafe { RegCloseKey(hkey) };

    if rc != 0 || buf_len == 0 {
        return None;
    }

    // Trim null terminator
    let len = buf_len as usize;
    let s = if len > 0 && buf[len - 1] == 0 {
        &buf[..len - 1]
    } else {
        &buf[..len]
    };
    String::from_utf8(s.to_vec()).ok().map(|s| s.trim().to_string())
}

// ── Sensor sampling ──────────────────────────────────────────────────────────

impl SystemState {
    /// Number of consecutive identical LHM responses before declaring stale.
    /// 20 ticks × 500ms = 10 seconds. A live system with dozens of sensors
    /// reporting decimal values cannot produce byte-identical JSON this long.
    const LHM_STALE_THRESHOLD: u32 = 20;

    pub(super) fn sample_sensors(&mut self) {
        // CPU utilization via NT kernel (fast, no WMI needed)
        self.sample_utilization_nt();

        // All other sensors via LibreHardwareMonitor HTTP
        if self.lhm_available {
            if let Some(data) = query_lhm_http() {
                // Staleness detection: if the raw JSON response is byte-identical
                // across many consecutive requests, LHM's sensor polling has
                // stopped and all values are frozen.
                if self.lhm_prev_hash != 0 && data.response_hash == self.lhm_prev_hash {
                    self.lhm_same_hash_ticks += 1;
                    if self.lhm_same_hash_ticks == Self::LHM_STALE_THRESHOLD {
                        warn!(
                            "LibreHardwareMonitor sensors appear stale \
                             (identical response for {:.0}s), sensor data unavailable until LHM recovers",
                            Self::LHM_STALE_THRESHOLD as f64 * 0.5
                        );
                        self.lhm_available = false;
                        // Keep lhm_prev_hash so retry logic can verify data has
                        // actually changed before re-enabling LHM.
                        self.lhm_same_hash_ticks = 0;
                        return;
                    }
                } else {
                    self.lhm_same_hash_ticks = 0;
                }
                self.lhm_prev_hash = data.response_hash;

                if let Some(temp) = data.temp_c {
                    self.temp_c = temp;
                }
                if let Some(fan) = data.fan_rpm {
                    self.fan_rpm = fan;
                    self.fan_detected = true;
                }
                if let Some(ppt) = data.ppt_watts {
                    self.ppt_watts = ppt;
                }
                // LHM reports per-physical-core clocks, but we track per-logical-core.
                // On hybrid CPUs (e.g. Intel P+E cores with HT), the count won't match.
                // Use LHM freqs when they match 1:1, otherwise fall back to NT API
                // which always reports per-logical-core.
                if data.per_core_freq.len() == self.cores.len() {
                    for (i, &freq) in data.per_core_freq.iter().enumerate() {
                        self.cores[i].freq_mhz = freq;
                    }
                } else {
                    self.sample_frequencies_nt();
                }
            } else {
                // LHM HTTP failed — may have been closed
                self.lhm_available = false;
                self.lhm_prev_hash = 0;
                self.lhm_same_hash_ticks = 0;
                warn!("LibreHardwareMonitor HTTP server lost, sensor data unavailable");
            }
        }
        // When LHM is unavailable, we only have CPU utilization (from NT).
        // No fallback for temp/power/freq/fan — better to show nothing than wrong data.

        // Periodically retry LHM if not available (every ~30s at 500ms interval).
        // When retrying after a staleness detection, verify the data has actually
        // changed before re-enabling — otherwise we'd cycle endlessly between
        // "stale → fallback → retry → re-enable → stale".
        if !self.lhm_available {
            static RETRY_COUNTER: std::sync::atomic::AtomicU32 =
                std::sync::atomic::AtomicU32::new(0);
            let count = RETRY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count % 60 == 0 {
                if let Some(data) = query_lhm_http() {
                    // Only re-enable if the response has changed since we last
                    // declared it stale (lhm_prev_hash holds the last stale hash).
                    if self.lhm_prev_hash == 0 || data.response_hash != self.lhm_prev_hash {
                        self.lhm_available = true;
                        self.lhm_prev_hash = 0;
                        self.lhm_same_hash_ticks = 0;
                        tracing::info!(
                            "LibreHardwareMonitor HTTP server detected, switching to full sensor data"
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn has_fan_sensor(&self) -> bool {
        self.fan_detected
    }

    fn sample_utilization_nt(&mut self) {
        let n = self.n_cores;
        if n == 0 {
            return;
        }

        let entry_size = std::mem::size_of::<ProcessorPerformanceInfo>();
        let buf_size = n * entry_size;
        let mut buf = vec![0u8; buf_size];
        let mut return_length: u32 = 0;

        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
                buf.as_mut_ptr() as *mut c_void,
                buf_size as u32,
                &mut return_length,
            )
        };

        if status != 0 {
            return;
        }

        let count = (return_length as usize) / entry_size;
        for i in 0..count.min(n) {
            let info = unsafe {
                &*(buf.as_ptr().add(i * entry_size) as *const ProcessorPerformanceInfo)
            };

            // kernel_time includes idle_time
            let total = (info.kernel_time + info.user_time) as u64;
            let idle = info.idle_time as u64;

            let core = &mut self.cores[i];
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

    fn sample_frequencies_nt(&mut self) {
        let n = self.n_cores;
        if n == 0 {
            return;
        }

        let entry_size = std::mem::size_of::<ProcessorPowerInfo>();
        let buf_size = n * entry_size;
        let mut buf = vec![0u8; buf_size];

        let status = unsafe {
            CallNtPowerInformation(
                PROCESSOR_INFORMATION_LEVEL,
                std::ptr::null(),
                0,
                buf.as_mut_ptr() as *mut c_void,
                buf_size as u32,
            )
        };

        if status != 0 {
            return;
        }

        for i in 0..n {
            let info =
                unsafe { &*(buf.as_ptr().add(i * entry_size) as *const ProcessorPowerInfo) };
            self.cores[i].freq_mhz = info.current_mhz;
        }
    }
}

// ── Wall clock ───────────────────────────────────────────────────────────────

pub fn wall_clock_hms() -> (u8, u8, u8) {
    let mut st = WinSystemTime {
        w_year: 0,
        w_month: 0,
        w_day_of_week: 0,
        w_day: 0,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    (st.w_hour as u8, st.w_minute as u8, st.w_second as u8)
}

// ── System info queries ──────────────────────────────────────────────────────

pub fn get_hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string())
}

pub fn get_cpu_model() -> String {
    // Read from registry — fast, no WMI needed
    read_registry_string(
        "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
        "ProcessorNameString",
    )
    .unwrap_or_else(|| "Unknown CPU".to_string())
}

pub fn get_max_freq() -> u32 {
    // Read from registry
    let subkey = "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0\0";
    let value_name = "~MHz\0";
    let mut hkey: HKEY = std::ptr::null_mut();

    let rc = unsafe {
        RegOpenKeyExA(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if rc != 0 {
        return get_max_freq_nt();
    }

    let mut mhz: u32 = 0;
    let mut buf_len = std::mem::size_of::<u32>() as u32;
    let mut value_type: u32 = 0;

    let rc = unsafe {
        RegQueryValueExA(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut value_type,
            &mut mhz as *mut u32 as *mut u8,
            &mut buf_len,
        )
    };
    unsafe { RegCloseKey(hkey) };

    if rc == 0 && mhz > 0 {
        mhz
    } else {
        get_max_freq_nt()
    }
}

fn get_max_freq_nt() -> u32 {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let entry_size = std::mem::size_of::<ProcessorPowerInfo>();
    let buf_size = n * entry_size;
    let mut buf = vec![0u8; buf_size];

    let status = unsafe {
        CallNtPowerInformation(
            PROCESSOR_INFORMATION_LEVEL,
            std::ptr::null(),
            0,
            buf.as_mut_ptr() as *mut c_void,
            buf_size as u32,
        )
    };

    if status == 0 && !buf.is_empty() {
        let info = unsafe { &*(buf.as_ptr() as *const ProcessorPowerInfo) };
        if info.max_mhz > 0 {
            return info.max_mhz;
        }
    }

    5000
}

pub fn get_scaling_driver() -> String {
    "n/a".to_string()
}

pub fn get_governor() -> String {
    "n/a".to_string()
}

pub fn get_total_ram_gb() -> f64 {
    let mut status = MemoryStatusEx {
        dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
        dw_memory_load: 0,
        ull_total_phys: 0,
        ull_avail_phys: 0,
        ull_total_page_file: 0,
        ull_avail_page_file: 0,
        ull_total_virtual: 0,
        ull_avail_virtual: 0,
        ull_avail_extended_virtual: 0,
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok != 0 {
        let gb = status.ull_total_phys as f64 / (1024.0 * 1024.0 * 1024.0);
        (gb * 10.0).round() / 10.0
    } else {
        0.0
    }
}
