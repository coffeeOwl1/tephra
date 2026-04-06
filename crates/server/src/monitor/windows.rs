use std::ffi::c_void;
use std::io::Read as _;

use serde::Deserialize;
use tracing::warn;

use super::SystemState;

const LHM_URL: &str = "http://localhost:8085/data.json";

// ── Platform initialization ──────────────────────────────────────────────────

pub(super) struct PlatformFields {
    pub lhm_available: bool,
    pub lhm_exe_path: Option<String>,
}

pub(super) fn init_platform() -> PlatformFields {
    let lhm_exe_path = find_lhm_exe_path();
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

    if let Some(ref path) = lhm_exe_path {
        tracing::info!("LHM auto-restart enabled: {path}");
        setup_lhm_restart_task(path);
    }

    PlatformFields {
        lhm_available,
        lhm_exe_path,
    }
}

/// Remove the LHM auto-restart scheduled task created during init.
pub fn cleanup_platform() {
    let _ = std::process::Command::new("schtasks")
        .args(["/delete", "/tn", "_tephra_lhm", "/f"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Find LHM's executable path from the running process.
///
/// Step 1: Use `tasklist` to confirm LHM is actually running (works on all
/// Windows versions including 11 24H2+ where `wmic` was removed).
/// Step 2: Try `wmic` first (older Windows), then fall back to well-known
/// install locations.
fn find_lhm_exe_path() -> Option<String> {
    // Step 1 — confirm the process is running via tasklist
    let tasklist = std::process::Command::new("tasklist")
        .args([
            "/FI",
            "IMAGENAME eq LibreHardwareMonitor.exe",
            "/FO",
            "CSV",
            "/NH",
        ])
        .output()
        .ok()?;

    let tasklist_out = String::from_utf8_lossy(&tasklist.stdout);
    let running = tasklist_out
        .lines()
        .any(|l| l.to_ascii_lowercase().contains("librehardwaremonitor.exe"));

    if !running {
        return None;
    }

    // Step 2a — try wmic (works on Windows 10 / Server 2019 and older)
    if let Some(path) = find_lhm_path_via_wmic() {
        return Some(path);
    }

    // Step 2b — fall back to common install locations
    let candidates = [
        r"C:\Program Files\LibreHardwareMonitor\LibreHardwareMonitor.exe",
        r"C:\Program Files (x86)\LibreHardwareMonitor\LibreHardwareMonitor.exe",
    ];

    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }

    // Step 2c — check the current user's home directory (portable installs)
    if let Ok(home) = std::env::var("USERPROFILE") {
        let home_candidate =
            format!(r"{home}\LibreHardwareMonitor\LibreHardwareMonitor.exe");
        if std::path::Path::new(&home_candidate).exists() {
            return Some(home_candidate);
        }
    }

    // We know it's running but couldn't locate the exe — return None so the
    // caller skips setting up the scheduled restart task.
    warn!(
        "LHM is running but executable path could not be determined; auto-restart disabled"
    );
    None
}

/// Try to resolve the LHM executable path via `wmic` (Windows 10 / Server
/// 2019 and older). Returns `None` if wmic is unavailable or produces no
/// output (e.g. Windows 11 24H2+ where wmic was removed).
fn find_lhm_path_via_wmic() -> Option<String> {
    let output = std::process::Command::new("wmic")
        .args([
            "process",
            "where",
            "name='LibreHardwareMonitor.exe'",
            "get",
            "ExecutablePath",
            "/value",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(path) = line.strip_prefix("ExecutablePath=") {
            let path = path.trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Create or update a scheduled task that can relaunch LHM in the user's
/// interactive session. This is needed because tephra runs as SYSTEM in
/// Session 0 and can't directly start GUI apps in the user's desktop.
///
/// The task must run as the logged-in user (not SYSTEM) with /IT so it
/// lands in the console session where LHM can access hardware sensors.
fn setup_lhm_restart_task(lhm_path: &str) {
    // Find the logged-in console user via `query user`
    let username = match find_console_user() {
        Some(u) => u,
        None => {
            warn!("No console user found, LHM auto-restart unavailable");
            return;
        }
    };

    // /ru <user> with /rl highest and /it specifies the task should run
    // as the logged-in user with interactive privilege, for tasks that
    // only run when the user is logged in.
    let result = std::process::Command::new("schtasks")
        .args([
            "/create",
            "/tn",
            "_tephra_lhm",
            "/tr",
            lhm_path,
            "/sc",
            "onlogon",
            "/ru",
            &username,
            "/rl",
            "highest",
            "/it",
            "/f",
        ])
        .stdin(std::process::Stdio::null())
        .output();

    match result {
        Ok(output) if output.status.success() => {
            tracing::info!("Created _tephra_lhm scheduled task (user: {username})");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to create _tephra_lhm task: {}", stderr.trim());
        }
        Err(e) => {
            warn!("Failed to run schtasks: {e}");
        }
    }
}

/// Find the username of the console session user via `query user`.
///
/// NOTE: The `query user` output format varies by locale and may fail on
/// non-English Windows or systems without Remote Desktop Services.
/// WTSEnumerateSessionsW would be more robust but adds significant FFI
/// complexity for a feature that only runs once at startup.
fn find_console_user() -> Option<String> {
    let output = std::process::Command::new("query")
        .arg("user")
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        // Format: " USERNAME  SESSIONNAME  ID  STATE  IDLE TIME  LOGON TIME"
        // The username may or may not have a leading ">" for the current session
        let line = line.trim_start_matches('>').trim();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("console") {
            return Some(parts[0].to_string());
        }
    }
    None
}

/// Kill a stale LHM process and relaunch it via the scheduled task.
fn restart_lhm() -> bool {
    tracing::info!("Attempting to restart LibreHardwareMonitor");

    // Kill the frozen process
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "LibreHardwareMonitor.exe"])
        .output();

    // Brief pause for the process to fully exit.
    // NOTE: This blocks the tokio executor thread. Keep the delay short.
    // A full async conversion would require changing the sample_sensors call chain.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Relaunch via scheduled task (runs in user's interactive session)
    let result = std::process::Command::new("schtasks")
        .args(["/run", "/tn", "_tephra_lhm"])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            tracing::info!("LibreHardwareMonitor restart triggered, waiting for sensor init");
            true
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to restart LHM: {}", stderr.trim());
            false
        }
        Err(e) => {
            warn!("Failed to run schtasks: {e}");
            false
        }
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
    let num_str: String = s.trim().chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    match num_str.parse() {
        Ok(v) => v,
        Err(_) => {
            tracing::debug!("failed to parse sensor value: {:?}", s);
            0.0
        }
    }
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

/// Recursive tree walker that collects sensor data from LHM nodes.
fn collect(node: &LhmNode, data: &mut LhmData) {
    if let (Some(ref stype), Some(ref val_str), Some(ref sid)) =
        (&node.sensor_type, &node.value, &node.sensor_id)
    {
        let id_lower = sid.to_lowercase();
        let is_cpu = id_lower.starts_with("/cpu")
            || id_lower.starts_with("/amdcpu")
            || id_lower.starts_with("/intelcpu");

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

/// Recursive tree walker that finds the maximum CPU temperature from LHM nodes.
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
    collect(&root, &mut data);

    // If we didn't find a package/tctl temp, fall back to highest CPU temp
    if data.temp_c.is_none() {
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
    use std::net::ToSocketAddrs;

    let url = url.strip_prefix("http://")?;
    let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
    let path = format!("/{path}");

    // TODO: migrate to async HTTP to avoid blocking the tokio executor
    let connect_timeout = std::time::Duration::from_millis(150);
    let io_timeout = std::time::Duration::from_millis(150);
    let addr = host_port.to_socket_addrs().ok()?.next()?;
    let stream = std::net::TcpStream::connect_timeout(&addr, connect_timeout).ok()?;
    stream.set_read_timeout(Some(io_timeout)).ok()?;
    stream.set_write_timeout(Some(io_timeout)).ok()?;

    use std::io::{Read, Write};
    let mut stream = stream;
    // HTTP/1.0 intentional: avoids chunked Transfer-Encoding so we can read until connection close
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: {host_port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;

    let mut response = Vec::new();
    stream.take(1_048_576).read_to_end(&mut response).ok()?;

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

#[link(name = "ntdll")]
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
    fn RegOpenKeyExW(
        hkey: HKEY,
        sub_key: *const u16,
        options: u32,
        sam_desired: u32,
        result: *mut HKEY,
    ) -> i32;
    fn RegQueryValueExW(
        hkey: HKEY,
        value_name: *const u16,
        reserved: *const u32,
        value_type: *mut u32,
        data: *mut u8,
        cb_data: *mut u32,
    ) -> i32;
    fn RegCloseKey(hkey: HKEY) -> i32;
}

const HKEY_LOCAL_MACHINE: HKEY = 0x80000002isize as HKEY;
const KEY_READ: u32 = 0x20019;

fn read_registry_string(subkey: &str, value_name: &str) -> Option<String> {
    let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(Some(0)).collect();
    let value_wide: Vec<u16> = value_name.encode_utf16().chain(Some(0)).collect();
    let mut hkey: HKEY = std::ptr::null_mut();

    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey_wide.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if rc != 0 {
        return None;
    }

    // Buffer sized in bytes; wide strings use 2 bytes per character
    let mut buf = vec![0u8; 1024];
    let mut buf_len = buf.len() as u32;
    let mut value_type: u32 = 0;

    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            value_wide.as_ptr(),
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

    // REG_SZ = 1; only interpret as string for string types
    if value_type != 1 {
        return None;
    }

    // Reinterpret byte buffer as u16 slice, trim null terminator
    let len_u16 = buf_len as usize / 2;
    let wide = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, len_u16) };
    let wide = if wide.last() == Some(&0) { &wide[..wide.len() - 1] } else { wide };
    String::from_utf16(wide).ok().map(|s| s.trim().to_string())
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
                             (identical response for {:.0}s)",
                            Self::LHM_STALE_THRESHOLD as f64 * 0.5
                        );
                        self.lhm_available = false;
                        self.fan_detected = false;
                        // Keep lhm_prev_hash so retry logic can verify data has
                        // actually changed before re-enabling LHM.
                        self.lhm_same_hash_ticks = 0;

                        // Auto-restart LHM if we have its path and haven't
                        // tried recently (at most once per 5 minutes).
                        self.try_restart_lhm();
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
                warn!("LibreHardwareMonitor HTTP server lost");
                self.try_restart_lhm();
            }
        }
        // When LHM is unavailable, we only have CPU utilization (from NT).
        // No fallback for temp/power/freq/fan — better to show nothing than wrong data.

        // Periodically retry LHM if not available (every ~30s at 500ms interval).
        // When retrying after a staleness detection, verify the data has actually
        // changed before re-enabling — otherwise we'd cycle endlessly between
        // "stale → fallback → retry → re-enable → stale".
        if !self.lhm_available {
            let count = self.lhm_retry_counter;
            self.lhm_retry_counter = self.lhm_retry_counter.wrapping_add(1);
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

    /// Minimum interval between LHM restart attempts.
    const LHM_RESTART_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

    fn try_restart_lhm(&mut self) {
        if self.lhm_exe_path.is_none() {
            return;
        }

        let now = std::time::Instant::now();
        if let Some(last) = self.lhm_last_restart {
            if now.duration_since(last) < Self::LHM_RESTART_COOLDOWN {
                tracing::debug!("LHM restart skipped (cooldown)");
                return;
            }
        }

        self.lhm_last_restart = Some(now);
        restart_lhm();
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

/// Returns the machine's NetBIOS name (COMPUTERNAME), which is limited to
/// 15 characters and uppercased. On domain-joined machines this may differ
/// from the DNS hostname.
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
    let subkey_wide: Vec<u16> = "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0"
        .encode_utf16().chain(Some(0)).collect();
    let value_wide: Vec<u16> = "~MHz"
        .encode_utf16().chain(Some(0)).collect();
    let mut hkey: HKEY = std::ptr::null_mut();

    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey_wide.as_ptr(),
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
        RegQueryValueExW(
            hkey,
            value_wide.as_ptr(),
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

    6000
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
