//! Orchestration: resolve drive, safety checks, prepare, run the seven tests, score.

use crate::platform::{self, RawDevice};
use crate::progress::Emitter;
use crate::report::*;
use crate::smart::{self, SmartData, SmartReading};
use crate::tests::{self as t, Ctx, Stab};
use crate::{score, util};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const EXIT_OK: i32 = 0;
pub const EXIT_BAD_ARGS: i32 = 2;
pub const EXIT_NEEDS_ADMIN: i32 = 3;
pub const EXIT_NOT_FOUND: i32 = 4;
pub const EXIT_REFUSED: i32 = 5;
pub const EXIT_NEEDS_FULL_DISK_ACCESS: i32 = 6;
pub const EXIT_INTERNAL: i32 = 10;

#[derive(Debug, Clone)]
pub struct EngineError {
    pub exit: i32,
    pub code: &'static str,
    pub params: Option<Params>,
}

impl EngineError {
    pub fn new(exit: i32, code: &'static str) -> Self {
        EngineError { exit, code, params: None }
    }
    pub fn with(mut self, k: &str, v: impl Into<serde_json::Value>) -> Self {
        self.params.get_or_insert_with(Params::new).insert(k.into(), v.into());
        self
    }
}

pub struct TestOptions {
    pub drive: String,
    pub mode: Mode,
    pub confirm_serial: Option<String>,
}

/// Fold SMART-derived identity/age data into a DriveInfo (fills gaps, never erases).
pub fn merge_smart(drive: &mut DriveInfo, d: &SmartData) {
    if drive.model.is_none() {
        drive.model = d.model.clone();
    }
    if drive.serial.is_none() {
        drive.serial = d.serial.clone();
    }
    if drive.firmware.is_none() {
        drive.firmware = d.firmware.clone();
    }
    if d.power_on_hours.is_some() {
        drive.power_on_hours = d.power_on_hours;
    }
    if d.rotation_rpm.is_some() {
        drive.rotation_rpm = d.rotation_rpm;
    }
    if drive.capacity_bytes == 0 {
        drive.capacity_bytes = d.capacity_bytes.unwrap_or(0);
    }
    drive.smart_supported = Some(true);
    match drive.kind {
        DriveKind::Usb if d.rotation_rpm.is_some() => drive.kind = DriveKind::Hdd,
        DriveKind::Unknown => {
            drive.kind = if d.protocol == smart::Protocol::Nvme {
                DriveKind::Nvme
            } else if d.rotation_rpm.is_some() {
                DriveKind::Hdd
            } else if d.is_ssd {
                DriveKind::Ssd
            } else {
                DriveKind::Unknown
            }
        }
        _ => {}
    }
}

/// `list` command: basic OS info, then SMART enrichment (parallel, best effort, unprivileged-safe).
pub fn list_enriched() -> Vec<DriveInfo> {
    let drives = platform::list_drives();
    let Some(sc) = smart::find_smartctl() else { return drives };
    let handles: Vec<_> = drives
        .into_iter()
        .map(|mut d| {
            let sc = sc.clone();
            std::thread::spawn(move || {
                let r = smart::read_smart(Some(&sc), &d, Duration::from_secs(10));
                match r.data {
                    Ok(s) => merge_smart(&mut d, &s),
                    Err(smart::SmartError::Unsupported) => d.smart_supported = Some(false),
                    Err(_) => {}
                }
                d
            })
        })
        .collect();
    let mut out: Vec<DriveInfo> = handles.into_iter().filter_map(|h| h.join().ok()).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn norm_serial(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
}

/// True when this process runs with root privileges (always false off Unix).
fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Error for a raw-device open that failed with permission denied (EPERM/EACCES).
/// Root on macOS that is still denied is blocked by TCC privacy protection, not by missing admin
/// rights: the app needs Full Disk Access. Everywhere else it means "needs administrator".
pub fn permission_denied_error(macos: bool, root: bool) -> EngineError {
    if macos && root {
        EngineError::new(EXIT_NEEDS_FULL_DISK_ACCESS, "error.needs_full_disk_access")
    } else {
        EngineError::new(EXIT_NEEDS_ADMIN, "error.needs_admin")
    }
}

/// Wipe confirmation: must equal one of the drive's serials; if the drive reports none, its id.
pub fn confirm_matches(confirm: &str, serials: &[&str], id: &str) -> bool {
    let c = norm_serial(confirm);
    if c.is_empty() {
        return false;
    }
    if serials.iter().any(|s| !s.trim().is_empty()) {
        serials.iter().any(|s| !s.trim().is_empty() && norm_serial(s) == c)
    } else {
        norm_serial(id) == c || norm_serial(&platform::normalize_id(id)) == c
    }
}

pub fn run_test(opts: &TestOptions, emit: Arc<Emitter>) -> Result<TestReport, EngineError> {
    let started = Instant::now();
    let started_at = util::iso_now();
    emit.progress(Phase::Prepare, 0.0, "progress.prepare.starting", None);

    let mut drive = platform::find_drive(&opts.drive).ok_or_else(|| EngineError::new(EXIT_NOT_FOUND, "error.drive_not_found"))?;
    let basic_serial = drive.serial.clone();

    // ---- safety gates that need no privileges
    if opts.mode == Mode::Wipe {
        if drive.is_system_disk {
            return Err(EngineError::new(EXIT_REFUSED, "error.system_disk_refused"));
        }
        if opts.confirm_serial.is_none() {
            return Err(EngineError::new(EXIT_REFUSED, "error.confirm_required"));
        }
        // Early check against the OS-reported serial when there is one.
        if let (Some(c), Some(s)) = (&opts.confirm_serial, &basic_serial) {
            if !confirm_matches(c, &[s.as_str()], &drive.id) && drive.serial.is_some() {
                // The SMART serial may still match (checked again below); only fail fast when smartctl is absent.
                if smart::find_smartctl().is_none() {
                    return Err(EngineError::new(EXIT_REFUSED, "error.confirm_mismatch"));
                }
            }
        }
    }

    // ---- privilege check: raw device must open READ-ONLY
    match RawDevice::open(&drive, false) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(permission_denied_error(cfg!(target_os = "macos"), is_root())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(EngineError::new(EXIT_NOT_FOUND, "error.drive_not_found")),
        Err(e) => return Err(EngineError::new(EXIT_INTERNAL, "error.cannot_open_device").with("detail", e.to_string())),
    }

    // ---- SMART baseline
    emit.progress(Phase::Prepare, 30.0, "progress.prepare.reading_smart", None);
    let smartctl = smart::find_smartctl();
    let reading: SmartReading = smart::read_smart(smartctl.as_deref(), &drive, Duration::from_secs(25));
    if let Ok(d) = &reading.data {
        merge_smart(&mut drive, d);
    } else if matches!(reading.data, Err(smart::SmartError::Unsupported)) {
        drive.smart_supported = Some(false);
    }

    if opts.mode == Mode::Wipe {
        let mut serials: Vec<&str> = Vec::new();
        if let Some(s) = &basic_serial {
            serials.push(s);
        }
        if let Some(s) = reading.data.as_ref().ok().and_then(|d| d.serial.as_deref()) {
            serials.push(s);
        }
        let c = opts.confirm_serial.as_deref().unwrap_or("");
        if !confirm_matches(c, &serials, &drive.id) {
            return Err(EngineError::new(EXIT_REFUSED, "error.confirm_mismatch"));
        }
        emit.progress(Phase::Prepare, 60.0, "progress.prepare.unmounting", None);
        platform::unmount(&drive).map_err(|e| EngineError::new(EXIT_INTERNAL, "error.unmount_failed").with("detail", e))?;
        // Writable handle check (also proves we can write raw after unmount).
        if let Err(e) = RawDevice::open(&drive, true) {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                return Err(permission_denied_error(cfg!(target_os = "macos"), is_root()));
            }
            return Err(EngineError::new(EXIT_INTERNAL, "error.cannot_open_device").with("detail", e.to_string()));
        }
    }
    emit.progress(Phase::Prepare, 100.0, "progress.prepare.ready", None);

    // ---- context, watchdog, heartbeat
    let stab = Arc::new(Stab::default());
    let ctx = Arc::new(Ctx { drive: drive.clone(), mode: opts.mode, emit: emit.clone(), smartctl: smartctl.clone(), smart: reading, stab: stab.clone() });
    let stop = Arc::new(AtomicBool::new(false));
    let hb = {
        let (emit, stop) = (emit.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                emit.heartbeat();
            }
        })
    };
    let wd = {
        let (drive, stab, stop) = (drive.clone(), stab.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut last = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                if last.elapsed() >= Duration::from_secs(5) {
                    last = Instant::now();
                    if platform::present(&drive) {
                        stab.mark_back();
                    } else {
                        stab.mark_gone();
                    }
                }
            }
        })
    };

    // ---- the tests
    let r_info = t::drive_info::run(&ctx);
    let r_smart = t::smart_health::run(&ctx);
    let selftest_handle = {
        let c = ctx.clone();
        std::thread::spawn(move || t::selftest_short::run(&c))
    };
    let r_surface = t::surface_read::run(&ctx);
    let r_selftest = selftest_handle
        .join()
        .unwrap_or_else(|_| TestResult::skipped(TestId::SelftestShort, vec![Issue::info("issue.selftest.could_not_start")]));
    let r_write = t::write_verify::run(&ctx);
    let r_cap = t::capacity_check::run(&ctx);
    let r_stab = t::stability::run(&ctx);

    stop.store(true, Ordering::Relaxed);
    let _ = hb.join();
    let _ = wd.join();

    emit.progress(Phase::Finish, 20.0, "progress.finish.scoring", None);
    let tests = vec![r_info, r_smart, r_selftest, r_surface, r_write, r_cap, r_stab];
    let report = build_report(drive, opts.mode, started_at, started.elapsed().as_millis() as u64, tests);
    emit.progress(Phase::Finish, 100.0, "progress.finish.done", None);
    Ok(report)
}

/// Pure report assembly (unit-tested).
pub fn build_report(drive: DriveInfo, mode: Mode, started_at: String, duration_ms: u64, tests: Vec<TestResult>) -> TestReport {
    let sc = score::overall_score(&tests);
    let issues = score::sorted_issues(&tests);
    let facts = score::Facts::from_tests(&tests);
    let has_critical = issues.iter().any(|i| i.severity == Severity::Critical);
    let suitability = score::suitability(sc, &facts, has_critical, drive.capacity_bytes);
    let cycles = None;
    let age = Age {
        power_on_hours: drive.power_on_hours,
        power_on_years: drive.power_on_hours.map(|h| util::round1(h as f64 / util::HOURS_PER_YEAR)),
        manufacture_date: drive.manufacture_date.clone(),
        power_cycles: tests
            .iter()
            .find(|t| t.id == TestId::SmartHealth)
            .and_then(|t| t.metrics.get("power_cycles"))
            .and_then(|v| v.as_u64())
            .or(cycles),
    };
    TestReport {
        schema_version: 1,
        engine_version: ENGINE_VERSION.to_string(),
        drive,
        mode,
        started_at,
        duration_ms,
        tests,
        score: sc,
        verdict: score::verdict(sc),
        issues,
        age,
        suitability,
        score_cap: score::cap_reasons(&facts).first().map(|(_, c)| *c),
        score_reasons: score::score_reasons(&facts, sc),
    }
}

pub fn version_json() -> serde_json::Value {
    let sc = smart::find_smartctl().and_then(|p| smart::smartctl_version(&p));
    serde_json::json!({"version": ENGINE_VERSION, "smartctl": sc})
}

pub fn out_path(p: &Option<String>) -> Option<PathBuf> {
    p.as_ref().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denied_mapping() {
        assert_eq!(permission_denied_error(true, true).code, "error.needs_full_disk_access");
        assert_eq!(permission_denied_error(true, true).exit, EXIT_NEEDS_FULL_DISK_ACCESS);
        assert_eq!(permission_denied_error(true, false).code, "error.needs_admin");
        assert_eq!(permission_denied_error(false, true).code, "error.needs_admin");
        assert_eq!(permission_denied_error(false, false).exit, EXIT_NEEDS_ADMIN);
    }

    #[test]
    fn confirm_rules() {
        assert!(confirm_matches("ABC 123", &["abc123"], "disk4"));
        assert!(!confirm_matches("", &["abc123"], "disk4"));
        assert!(!confirm_matches("disk4", &["abc123"], "disk4"));
        assert!(confirm_matches("disk4", &[], "disk4"));
        assert!(confirm_matches("/dev/sdb", &[], "/dev/sdb"));
        assert!(!confirm_matches("x", &[], "disk4"));
    }
}
