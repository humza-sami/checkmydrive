//! smartctl JSON parsing + analysis + scoring against realistic fixtures.

use checkmydrive_engine::engine::build_report;
use checkmydrive_engine::report::*;
use checkmydrive_engine::smart::{parse_smartctl_json, Protocol, SelfTestState, SmartData};
use checkmydrive_engine::tests::smart_health::analyze;
use serde_json::Value;

fn load(name: &str) -> Value {
    let p = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
fn parse(name: &str) -> SmartData {
    parse_smartctl_json(&load(name)).unwrap_or_else(|| panic!("{} should parse", name))
}
fn codes(issues: &[Issue]) -> Vec<&str> {
    issues.iter().map(|i| i.code.as_str()).collect()
}
fn drive(kind: DriveKind) -> DriveInfo {
    DriveInfo {
        id: "x".into(), path: "x".into(), model: None, serial: None, firmware: None, vendor: None,
        capacity_bytes: 1_000_000_000_000, kind, bus: None, rotation_rpm: None, removable: false,
        is_system_disk: false, mount_points: vec![], smart_supported: Some(true), power_on_hours: None, manufacture_date: None,
    }
}
/// Build a full report from just the SMART fixture (all other tests clean).
fn report_for(name: &str, kind: DriveKind) -> TestReport {
    let d = parse(name);
    let (m, issues) = analyze(&d);
    let mut dr = drive(kind);
    dr.power_on_hours = d.power_on_hours;
    let mut tests: Vec<TestResult> = TestId::ALL.iter().map(|i| TestResult::from_issues(*i, 0, Metrics::new(), vec![])).collect();
    let mut age = checkmydrive_engine::tests::drive_info::age_issues(d.power_on_hours);
    tests[0].issues.append(&mut age);
    tests[1] = TestResult::from_issues(TestId::SmartHealth, 0, m, issues);
    build_report(dr, Mode::Safe, "2026-01-01T00:00:00Z".into(), 1, tests)
}

#[test]
fn healthy_sata_ssd() {
    let d = parse("healthy_sata_ssd.json");
    assert_eq!(d.protocol, Protocol::Ata);
    assert_eq!(d.passed, Some(true));
    assert_eq!(d.model.as_deref(), Some("Samsung SSD 860 EVO 500GB"));
    assert_eq!(d.serial.as_deref(), Some("S3Z2NB0K123456A"));
    assert_eq!(d.power_on_hours, Some(12345));
    assert_eq!(d.power_cycles, Some(812));
    assert_eq!(d.reallocated, Some(0));
    assert_eq!(d.pending, Some(0));
    assert_eq!(d.crc_errors, Some(0));
    assert!(d.is_ssd && d.rotation_rpm.is_none());
    assert_eq!(d.wear_used_percent, Some(4.0));
    assert!(d.selftest_supported);
    assert_eq!(d.selftest_poll_minutes, Some(2));
    assert_eq!(d.selftest, SelfTestState::Passed);
    let r = report_for("healthy_sata_ssd.json", DriveKind::Ssd);
    assert!(r.issues.is_empty(), "{:?}", codes(&r.issues));
    assert_eq!(r.score, 100);
    assert_eq!(r.verdict, Verdict::Buy);
    assert!(r.suitability.os_install.ok);
    assert_eq!(r.suitability.safe_fill_percent, 100);
    assert_eq!(r.age.power_on_years, Some(1.4));
}

#[test]
fn healthy_nvme() {
    let d = parse("healthy_nvme.json");
    assert_eq!(d.protocol, Protocol::Nvme);
    assert_eq!(d.passed, Some(true));
    assert_eq!(d.power_on_hours, Some(4310));
    assert_eq!(d.power_cycles, Some(402));
    assert_eq!(d.wear_used_percent, Some(3.0));
    assert_eq!(d.available_spare, Some(100));
    assert_eq!(d.media_errors, Some(0));
    assert_eq!(d.temperature_c, Some(38));
    assert!(d.selftest_supported);
    assert_eq!(d.selftest, SelfTestState::Passed);
    let r = report_for("healthy_nvme.json", DriveKind::Nvme);
    assert_eq!(r.score, 100);
    assert_eq!(r.verdict, Verdict::Buy);
}

#[test]
fn failing_hdd_with_reallocated_and_pending() {
    let d = parse("failing_hdd.json");
    assert_eq!(d.rotation_rpm, Some(7200));
    assert!(!d.is_ssd);
    assert_eq!(d.reallocated, Some(2184));
    assert_eq!(d.pending, Some(16));
    assert_eq!(d.uncorrectable, Some(11)); // max(187 raw 11, 198 raw 8)
    assert_eq!(d.crc_errors, Some(3));
    assert_eq!(d.error_log_entries, Some(42));
    assert_eq!(d.wear_used_percent, None); // never reported for spinning disks
    let (m, issues) = analyze(&d);
    let c = codes(&issues);
    for want in ["issue.smart.reallocated_sectors", "issue.smart.pending_sectors", "issue.smart.uncorrectable_sectors", "issue.smart.crc_errors", "issue.smart.error_log_entries"] {
        assert!(c.contains(&want), "missing {} in {:?}", want, c);
    }
    assert_eq!(issues.iter().find(|i| i.code == "issue.smart.reallocated_sectors").unwrap().severity, Severity::Critical);
    assert_eq!(m["pending_sectors"], 16);
    let mut dr = drive(DriveKind::Hdd);
    dr.rotation_rpm = Some(7200);
    let r = report_for("failing_hdd.json", DriveKind::Hdd);
    assert!(r.score <= 40, "score {}", r.score);
    assert_eq!(r.verdict, Verdict::Avoid);
    assert!(!r.suitability.os_install.ok && !r.suitability.long_term_storage.ok);
    assert_eq!(r.suitability.safe_fill_percent, 0);
    assert_eq!(r.issues[0].severity, Severity::Critical);
    assert!(r.issues.iter().any(|i| i.code == "issue.age.high_hours"));
}

#[test]
fn usb_bridge_without_smart_does_not_parse() {
    assert!(parse_smartctl_json(&load("usb_bridge_no_smart.json")).is_none());
    // And the analysis wrapper turns that into an informational issue for USB flash.
    let i = checkmydrive_engine::tests::smart_health::unavailable_issue(&checkmydrive_engine::smart::SmartError::Unsupported, &drive(DriveKind::Usb));
    assert_eq!(i.code, "issue.smart.usb_bridge_blocked");
    assert_eq!(i.severity, Severity::Info);
    let i = checkmydrive_engine::tests::smart_health::unavailable_issue(&checkmydrive_engine::smart::SmartError::NoSmartctl, &drive(DriveKind::Usb));
    assert_eq!(i.code, "issue.smart.unavailable");
}

#[test]
fn worn_ssd_is_capped() {
    let d = parse("worn_ssd.json");
    assert_eq!(d.wear_used_percent, Some(97.0)); // SSD_Life_Left value 3
    let (_, issues) = analyze(&d);
    assert!(codes(&issues).contains(&"issue.smart.ssd_worn_out"));
    let r = report_for("worn_ssd.json", DriveKind::Ssd);
    assert!(r.score <= 40, "score {}", r.score);
    assert_eq!(r.verdict, Verdict::Avoid);
    assert_eq!(r.suitability.daily_use.level, SuitLevel::No);
}

#[test]
fn report_json_matches_contract_shape() {
    let r = report_for("healthy_sata_ssd.json", DriveKind::Ssd);
    let v = serde_json::to_value(&r).unwrap();
    for k in ["schema_version", "engine_version", "drive", "mode", "started_at", "duration_ms", "tests", "score", "verdict", "issues", "age", "suitability"] {
        assert!(v.get(k).is_some(), "missing {}", k);
    }
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["mode"], "safe");
    assert_eq!(v["verdict"], "buy");
    let ids: Vec<&str> = v["tests"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["drive_info", "smart_health", "selftest_short", "surface_read", "write_verify", "capacity_check", "stability"]);
    for k in ["os_install", "long_term_storage", "daily_use"] {
        assert!(v["suitability"][k]["level"].is_string() && v["suitability"][k]["reason_code"].is_string());
    }
    assert!(v["suitability"]["safe_fill_bytes"].is_u64() && v["suitability"]["safe_fill_percent"].is_u64());
    for k in ["power_on_hours", "power_on_years", "manufacture_date", "power_cycles"] {
        assert!(v["age"].get(k).is_some());
    }
    assert_eq!(v["drive"]["kind"], "ssd");
}
