//! smartctl discovery, execution (with USB-bridge fallbacks) and JSON parsing.

use crate::platform;
use crate::report::DriveInfo;
use crate::util::{run_with_timeout, CmdOutput};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Ata,
    Nvme,
    Scsi,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfTestState {
    Unknown,
    Running { percent_remaining: Option<u32> },
    Passed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone)]
pub struct SmartData {
    pub protocol: Protocol,
    pub passed: Option<bool>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub rotation_rpm: Option<u32>,
    pub is_ssd: bool,
    pub capacity_bytes: Option<u64>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub temperature_c: Option<i64>,
    pub reallocated: Option<u64>,
    pub pending: Option<u64>,
    pub uncorrectable: Option<u64>,
    pub crc_errors: Option<u64>,
    pub wear_used_percent: Option<f64>,
    pub available_spare: Option<u64>,
    pub spare_threshold: Option<u64>,
    pub media_errors: Option<u64>,
    pub critical_warning: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub error_log_entries: Option<u64>,
    pub selftest_supported: bool,
    pub selftest_poll_minutes: Option<u32>,
    pub selftest: SelfTestState,
}

impl Default for SmartData {
    fn default() -> Self {
        SmartData {
            protocol: Protocol::Unknown,
            passed: None,
            model: None,
            serial: None,
            firmware: None,
            rotation_rpm: None,
            is_ssd: false,
            capacity_bytes: None,
            power_on_hours: None,
            power_cycles: None,
            temperature_c: None,
            reallocated: None,
            pending: None,
            uncorrectable: None,
            crc_errors: None,
            wear_used_percent: None,
            available_spare: None,
            spare_threshold: None,
            media_errors: None,
            critical_warning: None,
            unsafe_shutdowns: None,
            error_log_entries: None,
            selftest_supported: false,
            selftest_poll_minutes: None,
            selftest: SelfTestState::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartError {
    /// smartctl binary not found.
    NoSmartctl,
    /// Device could not be opened for lack of privileges.
    NeedsAdmin,
    /// Device answered but exposes no usable SMART data (e.g. USB bridge, flash stick).
    Unsupported,
    /// smartctl hung / timed out on every attempt.
    Timeout,
}

#[derive(Debug, Clone)]
pub struct SmartReading {
    pub data: Result<SmartData, SmartError>,
    /// The `-d` type that worked (None = smartctl auto-detect).
    pub device_type: Option<String>,
    pub attempts: u32,
}

// ---------------------------------------------------------------- discovery

pub fn find_smartctl() -> Option<PathBuf> {
    let name = platform::smartctl_exe_name();
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("CHECKMYDRIVE_SMARTCTL") {
        cands.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join(name));
            cands.push(dir.join("resources").join(name));
            // macOS app bundle layout: Contents/MacOS/<exe> -> Contents/Resources
            if let Some(contents) = dir.parent() {
                cands.push(contents.join("Resources").join(name));
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&paths) {
            cands.push(d.join(name));
        }
    }
    for d in platform::smartctl_dirs() {
        cands.push(d.join(name));
    }
    // Developer convenience: vendored copy next to the sources (debug builds only).
    #[cfg(debug_assertions)]
    {
        let os = if cfg!(target_os = "macos") { "macos" } else if cfg!(windows) { "windows" } else { "linux" };
        cands.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor").join(os).join(name));
    }
    cands.into_iter().find(|p| p.is_file())
}

pub fn smartctl_version(path: &Path) -> Option<String> {
    let o = run_with_timeout(path, &["--version"], Duration::from_secs(5)).ok()?;
    // "smartctl 7.5 2025-04-30 r5714 [...]"
    o.stdout.lines().next()?.split_whitespace().nth(1).map(String::from)
}

// ---------------------------------------------------------------- parsing

fn u(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
}
fn path<'a>(v: &'a Value, p: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for k in p {
        cur = cur.get(*k)?;
    }
    Some(cur)
}
fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Combined 128-bit NVMe counters can exceed u64 in theory; saturate.
fn nvme_u(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(u)
}

/// Parse one `smartctl -a -j` document. `available` is false when it carries no health data at all.
pub fn parse_smartctl_json(root: &Value) -> Option<SmartData> {
    let mut d = SmartData::default();
    let protocol = path(root, &["device", "protocol"]).and_then(|x| x.as_str()).unwrap_or("");

    d.model = s(root, "model_name").or_else(|| s(root, "scsi_model_name")).or_else(|| s(root, "model_family"));
    d.serial = s(root, "serial_number");
    d.firmware = s(root, "firmware_version").or_else(|| s(root, "scsi_revision"));
    d.capacity_bytes = path(root, &["user_capacity", "bytes"]).and_then(u).or_else(|| path(root, &["nvme_total_capacity"]).and_then(u));
    d.passed = path(root, &["smart_status", "passed"]).and_then(|x| x.as_bool());
    d.power_on_hours = path(root, &["power_on_time", "hours"]).and_then(u);
    d.power_cycles = root.get("power_cycle_count").and_then(u);
    d.temperature_c = path(root, &["temperature", "current"]).and_then(|x| x.as_i64());
    d.rotation_rpm = root.get("rotation_rate").and_then(u).map(|r| r as u32);
    if let Some(r) = d.rotation_rpm {
        if r == 0 {
            d.rotation_rpm = None;
            d.is_ssd = true;
        }
    }

    let mut have_health = d.passed.is_some();

    if let Some(log) = root.get("nvme_smart_health_information_log") {
        have_health = true;
        d.protocol = Protocol::Nvme;
        d.is_ssd = true;
        d.critical_warning = nvme_u(log, "critical_warning");
        d.available_spare = nvme_u(log, "available_spare");
        d.spare_threshold = nvme_u(log, "available_spare_threshold");
        d.wear_used_percent = log.get("percentage_used").and_then(|x| x.as_f64());
        d.media_errors = nvme_u(log, "media_errors");
        d.error_log_entries = nvme_u(log, "num_err_log_entries");
        d.unsafe_shutdowns = nvme_u(log, "unsafe_shutdowns");
        d.power_cycles = d.power_cycles.or_else(|| nvme_u(log, "power_cycles"));
        d.power_on_hours = d.power_on_hours.or_else(|| nvme_u(log, "power_on_hours"));
        if d.temperature_c.is_none() {
            d.temperature_c = log.get("temperature").and_then(|x| x.as_i64());
        }
        if d.passed.is_none() {
            // NVMe overall health = critical_warning == 0
            d.passed = d.critical_warning.map(|c| c == 0);
        }
    }

    if let Some(table) = path(root, &["ata_smart_attributes", "table"]).and_then(|x| x.as_array()) {
        have_health = true;
        d.protocol = Protocol::Ata;
        parse_ata_attributes(table, &mut d);
    }
    if let Some(n) = path(root, &["ata_smart_error_log", "summary", "count"]).and_then(u) {
        d.error_log_entries = Some(n);
    }
    if root.get("ata_smart_data").is_some() {
        if d.protocol == Protocol::Unknown {
            d.protocol = Protocol::Ata;
        }
        d.selftest_supported = path(root, &["ata_smart_data", "capabilities", "self_tests_supported"]).and_then(|x| x.as_bool()).unwrap_or(false);
        d.selftest_poll_minutes = path(root, &["ata_smart_data", "self_test", "polling_minutes", "short"]).and_then(u).map(|x| x as u32);
        if let Some(st) = path(root, &["ata_smart_data", "self_test", "status"]) {
            d.selftest = parse_ata_selftest(st);
        }
    }

    if root.get("scsi_grown_defect_list").is_some() || root.get("scsi_error_counter_log").is_some() {
        if d.protocol == Protocol::Unknown {
            d.protocol = Protocol::Scsi;
        }
        have_health = true;
        if let Some(g) = root.get("scsi_grown_defect_list").and_then(u) {
            d.reallocated = Some(g);
        }
        if let Some(c) = path(root, &["scsi_error_counter_log", "read", "total_uncorrected_errors"]).and_then(u) {
            d.uncorrectable = Some(c);
        }
        if d.power_cycles.is_none() {
            d.power_cycles = path(root, &["scsi_start_stop_cycle_counter", "accumulated_start_stop_cycles"]).and_then(u);
        }
    }

    if root.get("nvme_self_test_log").is_some() {
        d.selftest = parse_nvme_selftest(root);
    }
    if let Some(self_test_cap) = path(root, &["nvme_controller_capabilities", "self_test"]).and_then(|x| x.as_bool()) {
        d.selftest_supported = self_test_cap;
    }

    if protocol == "NVMe" && d.protocol == Protocol::Unknown {
        d.protocol = Protocol::Nvme;
    }
    if !have_health {
        return None;
    }
    Some(d)
}

fn raw_val(a: &Value) -> Option<u64> {
    path(a, &["raw", "value"]).and_then(u)
}

pub fn parse_ata_attributes(table: &[Value], d: &mut SmartData) {
    let mut wear: Option<f64> = None;
    let mut note = |w: f64| {
        let w = w.clamp(0.0, 100.0);
        wear = Some(wear.map_or(w, |c| c.max(w)));
    };
    for a in table {
        let id = a.get("id").and_then(u).unwrap_or(0);
        let name = a.get("name").and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
        let value = a.get("value").and_then(|x| x.as_f64());
        let raw = raw_val(a);
        match id {
            5 => d.reallocated = raw,
            9 => {
                if d.power_on_hours.is_none() {
                    d.power_on_hours = raw;
                }
            }
            12 => {
                if d.power_cycles.is_none() {
                    d.power_cycles = raw;
                }
            }
            // 187 Reported_Uncorrect and 198 Offline_Uncorrectable both count as uncorrectable.
            187 | 198 => d.uncorrectable = match (d.uncorrectable, raw) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
            197 => d.pending = raw,
            199 => d.crc_errors = raw,
            194 | 190 if d.temperature_c.is_none() => {
                d.temperature_c = a.get("raw").and_then(|r| r.get("value")).and_then(|x| x.as_i64()).map(|t| t & 0xFF);
            }
            _ => {}
        }
        // SSD wear indicators (vendor specific). Normalised VALUE counts down from 100.
        if name.contains("life_used") || name.contains("rated_life_used") {
            if let Some(r) = raw {
                note(r as f64);
            }
        } else if name.contains("wear_leveling") || name.contains("wearout") || name.contains("life_left") || name.contains("lifetime_remain") || name.contains("remaining_life") {
            if let Some(v) = value {
                note(100.0 - v);
            }
        }
    }
    if d.is_ssd || d.rotation_rpm.is_none() {
        // Only trust wear on non-rotating media.
        d.wear_used_percent = wear;
    }
}

fn parse_ata_selftest(st: &Value) -> SelfTestState {
    let val = st.get("value").and_then(u).unwrap_or(0);
    match val >> 4 {
        0xF => SelfTestState::Running { percent_remaining: Some(((val & 0xF) * 10) as u32) },
        0 => SelfTestState::Passed,
        1 | 2 => SelfTestState::Aborted,
        _ => SelfTestState::Failed,
    }
}

fn parse_nvme_selftest(root: &Value) -> SelfTestState {
    let log = &root["nvme_self_test_log"];
    let cur = path(log, &["current_self_test_operation", "value"]).and_then(u).unwrap_or(0);
    if cur != 0 {
        let pct = log.get("current_self_test_completion_percent").and_then(u).map(|p| 100u32.saturating_sub(p as u32));
        return SelfTestState::Running { percent_remaining: pct };
    }
    let first = log.get("table").and_then(|t| t.as_array()).and_then(|t| t.first());
    match first.and_then(|e| path(e, &["self_test_result", "value"])).and_then(u) {
        Some(0) => SelfTestState::Passed,
        Some(1..=4) => SelfTestState::Aborted,
        Some(0xF) | None => SelfTestState::Unknown,
        Some(_) => SelfTestState::Failed,
    }
}

// ---------------------------------------------------------------- running

fn exit_bits(root: &Value) -> u64 {
    path(root, &["smartctl", "exit_status"]).and_then(u).unwrap_or(0)
}

fn messages_text(root: &Value) -> String {
    path(root, &["smartctl", "messages"])
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|m| m.get("string").and_then(|s| s.as_str())).collect::<Vec<_>>().join(" | ").to_lowercase())
        .unwrap_or_default()
}

fn looks_like_permission(text: &str) -> bool {
    text.contains("permission denied") || text.contains("operation not permitted") || text.contains("access is denied") || text.contains("administrator") || text.contains("must be root")
}

pub fn run_smartctl(smartctl: &Path, args: &[&str], timeout: Duration) -> Option<(CmdOutput, Option<Value>)> {
    let o = run_with_timeout(smartctl, args, timeout).ok()?;
    let v = serde_json::from_str::<Value>(&o.stdout).ok();
    Some((o, v))
}

/// Candidate `-d` types, in order, for a drive.
pub fn device_type_candidates(drive: &DriveInfo) -> Vec<Option<&'static str>> {
    use crate::report::DriveKind::*;
    match (drive.kind, drive.bus.as_deref()) {
        (Nvme, _) => vec![None, Some("nvme")],
        (Usb, _) | (_, Some("usb")) | (Sd, _) => {
            vec![None, Some("sat"), Some("auto"), Some("sntjmicron"), Some("sntrealtek"), Some("sntasmedia"), Some("scsi")]
        }
        _ => vec![None, Some("sat"), Some("auto")],
    }
}

/// Read SMART with fallbacks. `budget` bounds the total time spent.
pub fn read_smart(smartctl: Option<&Path>, drive: &DriveInfo, budget: Duration) -> SmartReading {
    let Some(sc) = smartctl else {
        return SmartReading { data: Err(SmartError::NoSmartctl), device_type: None, attempts: 0 };
    };
    let target = platform::smartctl_target(drive);
    let start = Instant::now();
    let mut attempts = 0;
    let mut saw_timeout = false;
    for dt in device_type_candidates(drive) {
        let left = budget.saturating_sub(start.elapsed());
        if left < Duration::from_secs(1) {
            break;
        }
        attempts += 1;
        let mut args: Vec<&str> = vec!["-a", "-j"];
        if let Some(t) = dt {
            args.push("-d");
            args.push(t);
        }
        args.push(&target);
        let per = left.min(Duration::from_secs(12));
        let Some((out, json)) = run_smartctl(sc, &args, per) else { continue };
        if out.timed_out {
            saw_timeout = true;
            continue;
        }
        if let Some(root) = &json {
            let text = messages_text(root);
            if exit_bits(root) & 2 != 0 && looks_like_permission(&text) || (json.is_none() && looks_like_permission(&out.stderr.to_lowercase())) {
                return SmartReading { data: Err(SmartError::NeedsAdmin), device_type: None, attempts };
            }
            if let Some(d) = parse_smartctl_json(root) {
                return SmartReading { data: Ok(d), device_type: dt.map(String::from), attempts };
            }
        } else if looks_like_permission(&out.stderr.to_lowercase()) {
            return SmartReading { data: Err(SmartError::NeedsAdmin), device_type: None, attempts };
        }
    }
    let err = if saw_timeout && attempts > 0 && start.elapsed() >= budget { SmartError::Timeout } else { SmartError::Unsupported };
    SmartReading { data: Err(err), device_type: None, attempts }
}

/// Start a short self-test. Returns false when the drive refuses / does not support it.
pub fn start_short_selftest(smartctl: &Path, drive: &DriveInfo, device_type: Option<&str>) -> bool {
    let target = platform::smartctl_target(drive);
    let mut args: Vec<&str> = vec!["-t", "short", "-j"];
    if let Some(t) = device_type {
        args.push("-d");
        args.push(t);
    }
    args.push(&target);
    match run_smartctl(smartctl, &args, Duration::from_secs(15)) {
        Some((o, Some(root))) => {
            if o.timed_out {
                return false;
            }
            let out_text = path(&root, &["smartctl", "output"])
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|l| l.as_str()).collect::<Vec<_>>().join("\n").to_lowercase())
                .unwrap_or_default();
            if out_text.contains("begun") {
                return true;
            }
            let t = messages_text(&root);
            exit_bits(&root) & 0b0000_0111 == 0 && !(t.contains("not supported") || t.contains("unsupported") || t.contains("unknown"))
        }
        _ => false,
    }
}

/// Poll the current self-test state.
pub fn poll_selftest(smartctl: &Path, drive: &DriveInfo, device_type: Option<&str>) -> Option<SelfTestState> {
    let target = platform::smartctl_target(drive);
    let mut args: Vec<&str> = vec!["-a", "-j"];
    if let Some(t) = device_type {
        args.push("-d");
        args.push(t);
    }
    args.push(&target);
    let (o, root) = run_smartctl(smartctl, &args, Duration::from_secs(15))?;
    if o.timed_out {
        return None;
    }
    let d = parse_smartctl_json(&root?)?;
    Some(d.selftest)
}

pub fn abort_selftest(smartctl: &Path, drive: &DriveInfo, device_type: Option<&str>) {
    let target = platform::smartctl_target(drive);
    let mut args: Vec<&str> = vec!["-X"];
    if let Some(t) = device_type {
        args.push("-d");
        args.push(t);
    }
    args.push(&target);
    let _ = run_with_timeout(smartctl, &args, Duration::from_secs(10));
}
