//! End-to-end exercise of the test modules against file-backed "devices" (no real disks, no root).

use checkmydrive_engine::progress::Emitter;
use checkmydrive_engine::report::*;
use checkmydrive_engine::smart::{SmartError, SmartReading};
use checkmydrive_engine::tests::{capacity_check, surface_read, write_verify, Ctx, Stab};
use std::sync::Arc;
use std::time::Duration;

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("cmd-test-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ctx(path: &str, size: u64, mode: Mode, mounts: Vec<String>) -> Ctx {
    Ctx {
        drive: DriveInfo {
            id: "fake".into(), path: path.into(), model: Some("Fake".into()), serial: Some("S".into()), firmware: None, vendor: None,
            capacity_bytes: size, kind: DriveKind::Ssd, bus: None, rotation_rpm: None, removable: true, is_system_disk: false,
            mount_points: mounts, smart_supported: None, power_on_hours: None, manufacture_date: None,
        },
        mode,
        emit: Arc::new(Emitter::new(None).unwrap()),
        smartctl: None,
        smart: SmartReading { data: Err(SmartError::NoSmartctl), device_type: None, attempts: 0 },
        stab: Arc::new(Stab::default()),
    }
}

fn sparse(dir: &std::path::Path, name: &str, size: u64) -> String {
    let p = dir.join(name);
    let f = std::fs::File::create(&p).unwrap();
    f.set_len(size).unwrap();
    p.to_string_lossy().to_string()
}

fn m(r: &TestResult, k: &str) -> serde_json::Value {
    r.metrics.get(k).cloned().unwrap_or_default()
}

#[test]
fn surface_read_on_file_backed_device() {
    let d = tmpdir("sr");
    let size = 256 * 1024 * 1024;
    let p = sparse(&d, "disk.img", size);
    let c = ctx(&p, size, Mode::Safe, vec![]);
    let r = surface_read::run_with(&c, Duration::from_millis(500));
    assert_eq!(m(&r, "read_errors"), 0, "{:?}", r.issues);
    assert!(m(&r, "samples_total").as_u64().unwrap() >= 512);
    assert!(m(&r, "sequential_bytes").as_u64().unwrap() > 0);
    assert_eq!(r.status, Status::Pass);
    std::fs::remove_dir_all(d).ok();
}

#[test]
fn safe_write_verify_uses_only_its_temp_file_and_cleans_up() {
    let d = tmpdir("wv");
    let mount = d.to_string_lossy().to_string();
    // a pre-existing user file must remain untouched
    std::fs::write(d.join("keep.txt"), b"precious").unwrap();
    let c = ctx("/dev/null", 0, Mode::Safe, vec![mount]);
    let r = write_verify::run_with(&c, 16 * 1024 * 1024);
    assert_eq!(r.status, Status::Pass, "{:?}", r.issues);
    assert_eq!(m(&r, "verify_mismatches"), 0);
    assert!(m(&r, "bytes_written").as_u64().unwrap() >= 8 * 1024 * 1024);
    let left: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(left, vec!["keep.txt".to_string()], "temp file must be deleted");
    assert_eq!(std::fs::read(d.join("keep.txt")).unwrap(), b"precious");
    std::fs::remove_dir_all(d).ok();
}

#[test]
fn safe_write_verify_without_mount_is_skipped() {
    let c = ctx("/dev/null", 0, Mode::Safe, vec![]);
    let r = write_verify::run_with(&c, 16 * 1024 * 1024);
    assert_eq!(r.status, Status::Skipped);
    assert_eq!(r.issues[0].code, "issue.write_verify.no_writable_volume");
}

#[test]
fn wipe_mode_raw_write_verify_and_capacity_check_pass_on_honest_device() {
    let d = tmpdir("raw");
    let size = 512 * 1024 * 1024;
    let p = sparse(&d, "disk.img", size);
    let c = ctx(&p, size, Mode::Wipe, vec![]);
    let r = write_verify::run_with(&c, 0);
    assert_eq!(r.status, Status::Pass, "{:?}", r.issues);
    assert_eq!(m(&r, "verify_mismatches"), 0);
    let r = capacity_check::run(&c);
    assert_eq!(r.status, Status::Pass, "{:?}", r.issues);
    assert_eq!(m(&r, "fake_capacity"), false);
    assert!(m(&r, "sectors_tested").as_u64().unwrap() >= 256);
    std::fs::remove_dir_all(d).ok();
}

#[test]
fn capacity_check_skipped_in_safe_mode() {
    let c = ctx("/dev/null", 0, Mode::Safe, vec![]);
    let r = capacity_check::run(&c);
    assert_eq!(r.status, Status::Skipped);
    assert_eq!(r.issues[0].code, "issue.capacity_check.needs_wipe_mode");
    assert_eq!(r.issues[0].severity, Severity::Info);
}
