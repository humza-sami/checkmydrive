//! Pure scoring functions: per-issue deductions, hard caps, verdict, suitability.

use crate::report::*;
use serde_json::{json, Value};

pub const CAP_SMART_FAILED: u8 = 10;
pub const CAP_FAKE_CAPACITY: u8 = 5;
pub const CAP_DISCONNECTED: u8 = 25;
pub const CAP_PENDING: u8 = 45;
pub const CAP_REALLOCATED: u8 = 70;
pub const CAP_REALLOCATED_MANY: u8 = 40;
pub const CAP_WEAR: u8 = 40;
pub const CAP_READ_ERRORS: u8 = 30;

/// Points removed for one issue: severity base + a capped magnitude term.
pub fn deduction(issue: &Issue) -> f64 {
    match issue.severity {
        Severity::Info => 0.0,
        Severity::Warn => 8.0 + (issue.magnitude() / 5.0).min(12.0),
        Severity::Critical => 25.0 + (issue.magnitude() / 10.0).min(15.0),
    }
}

pub fn test_score(issues: &[Issue]) -> u8 {
    let d: f64 = issues.iter().map(deduction).sum();
    (100.0 - d).round().clamp(0.0, 100.0) as u8
}

pub fn status_from_issues(issues: &[Issue]) -> Status {
    if issues.iter().any(|i| i.severity == Severity::Critical) {
        Status::Fail
    } else if issues.iter().any(|i| i.severity == Severity::Warn) {
        Status::Warn
    } else {
        Status::Pass
    }
}

/// Facts extracted from test metrics that drive the hard caps and suitability.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub smart_failed: bool,
    pub fake_capacity: bool,
    pub disconnected: bool,
    pub pending_or_uncorrectable: u64,
    pub reallocated: u64,
    pub wear_used_percent: Option<f64>,
    pub read_errors: u64,
    pub p99_latency_ok: bool,
    /// Sequential read speed is high enough to run an operating system from the drive.
    pub os_speed_ok: bool,
    /// capacity_check ran and passed (so the claimed capacity is proven, not just assumed).
    pub capacity_verified: bool,
}

/// Fallback for reports without the `os_speed_ok` metric: judge `read_mbps_avg` (0 = not measured).
const OS_MIN_MBPS_FALLBACK: f64 = 30.0;

fn metric<'a>(tests: &'a [TestResult], id: TestId, key: &str) -> Option<&'a Value> {
    tests.iter().find(|t| t.id == id).and_then(|t| t.metrics.get(key))
}
fn metric_u64(tests: &[TestResult], id: TestId, key: &str) -> u64 {
    metric(tests, id, key).and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64))).unwrap_or(0)
}

impl Facts {
    pub fn from_tests(tests: &[TestResult]) -> Facts {
        use TestId::*;
        Facts {
            smart_failed: metric(tests, SmartHealth, "smart_passed").and_then(|v| v.as_bool()) == Some(false),
            fake_capacity: metric(tests, CapacityCheck, "fake_capacity").and_then(|v| v.as_bool()) == Some(true),
            disconnected: metric(tests, Stability, "device_disconnected").and_then(|v| v.as_bool()) == Some(true),
            pending_or_uncorrectable: metric_u64(tests, SmartHealth, "pending_sectors")
                + metric_u64(tests, SmartHealth, "uncorrectable_sectors"),
            reallocated: metric_u64(tests, SmartHealth, "reallocated_sectors"),
            wear_used_percent: metric(tests, SmartHealth, "wear_used_percent").and_then(|v| v.as_f64()),
            read_errors: metric_u64(tests, SurfaceRead, "read_errors")
                + metric_u64(tests, WriteVerify, "verify_mismatches")
                + metric_u64(tests, WriteVerify, "write_errors"),
            p99_latency_ok: metric(tests, SurfaceRead, "p99_latency_ok").and_then(|v| v.as_bool()).unwrap_or(true),
            os_speed_ok: metric(tests, SurfaceRead, "os_speed_ok").and_then(|v| v.as_bool()).unwrap_or_else(|| {
                let mbps = metric(tests, SurfaceRead, "read_mbps_avg").and_then(|v| v.as_f64()).unwrap_or(0.0);
                mbps <= 0.0 || mbps >= OS_MIN_MBPS_FALLBACK
            }),
            capacity_verified: tests.iter().any(|t| t.id == CapacityCheck && t.status == Status::Pass),
        }
    }
}

/// Every hard cap that applies, as (reason code, cap), lowest cap first (stable for equal caps).
/// The codes are i18n keys (`score.reason.*`, see engine/I18N_KEYS.md).
pub fn cap_reasons(f: &Facts) -> Vec<(&'static str, u8)> {
    let mut v: Vec<(&'static str, u8)> = Vec::new();
    if f.smart_failed {
        v.push(("score.reason.smart_failed", CAP_SMART_FAILED));
    }
    if f.fake_capacity {
        v.push(("score.reason.fake_capacity", CAP_FAKE_CAPACITY));
    }
    if f.disconnected {
        v.push(("score.reason.disconnected", CAP_DISCONNECTED));
    }
    if f.pending_or_uncorrectable > 0 {
        v.push(("score.reason.unreadable", CAP_PENDING));
    }
    if f.reallocated > 100 {
        v.push(("score.reason.many_reallocated", CAP_REALLOCATED_MANY));
    } else if f.reallocated > 0 {
        v.push(("score.reason.reallocated", CAP_REALLOCATED));
    }
    if f.wear_used_percent.map(|w| w >= 90.0).unwrap_or(false) {
        v.push(("score.reason.worn_out", CAP_WEAR));
    }
    if f.read_errors > 0 {
        v.push(("score.reason.read_errors", CAP_READ_ERRORS));
    }
    v.sort_by_key(|(_, cap)| *cap);
    v
}

/// Lowest applicable hard cap (100 when none apply).
pub fn hard_cap(f: &Facts) -> u8 {
    cap_reasons(f).first().map(|(_, c)| *c).unwrap_or(100)
}

/// Why the score is what it is: the caps that apply (worst first); when no cap applies but the score is
/// below 100 because of the individual findings, the generic `score.reason.findings`. Empty at 100.
pub fn score_reasons(f: &Facts, score: u8) -> Vec<String> {
    let mut v: Vec<String> = cap_reasons(f).into_iter().map(|(c, _)| c.to_string()).collect();
    if v.is_empty() && score < 100 {
        v.push("score.reason.findings".to_string());
    }
    v
}

pub fn overall_score(tests: &[TestResult]) -> u8 {
    let all: Vec<Issue> = tests.iter().flat_map(|t| t.issues.iter().cloned()).collect();
    let base = test_score(&all);
    base.min(hard_cap(&Facts::from_tests(tests)))
}

pub fn verdict(score: u8) -> Verdict {
    if score >= 80 {
        Verdict::Buy
    } else if score >= 50 {
        Verdict::Caution
    } else {
        Verdict::Avoid
    }
}

/// Score-only table, used as a conservative fallback when an unexplained critical issue exists.
pub fn safe_fill_percent_for_score(score: u8) -> u8 {
    match score {
        80..=100 => 90,
        65..=79 => 60,
        50..=64 => 30,
        _ => 0,
    }
}

/// How much of the capacity we would fill. Driven by real problems (bad/unreadable regions, fake
/// capacity, errors, wear), NOT by speed or by the numeric score: a slow drive with no errors that
/// passed capacity_check is 100% safe to fill. Without a passed capacity_check (safe mode) the claimed
/// size is unproven, so it stays at 90.
pub fn safe_fill_percent(f: &Facts, score: u8, has_critical: bool) -> u8 {
    if f.fake_capacity
        || f.smart_failed
        || f.disconnected
        || f.pending_or_uncorrectable > 0
        || f.read_errors > 0
        || f.reallocated > 100
    {
        return 0;
    }
    let mut pct: u8 = if f.capacity_verified { 100 } else { 90 };
    if f.reallocated > 0 {
        pct = pct.min(60);
    }
    if f.wear_used_percent.map(|w| w >= 90.0).unwrap_or(false) {
        pct = pct.min(30);
    }
    if has_critical || score < 50 {
        pct = pct.min(safe_fill_percent_for_score(score));
    }
    pct
}

/// All issues sorted critical -> info (stable within a severity).
pub fn sorted_issues(tests: &[TestResult]) -> Vec<Issue> {
    let mut all: Vec<Issue> = tests.iter().flat_map(|t| t.issues.iter().cloned()).collect();
    all.sort_by_key(|i| std::cmp::Reverse(i.severity));
    all
}

fn suit(level: SuitLevel, code: &str, score: u8) -> Suitability {
    let mut p = Params::new();
    p.insert("score".into(), json!(score));
    Suitability { ok: level == SuitLevel::Good, level, reason_code: code.to_string(), params: Some(p) }
}

pub fn suitability(score: u8, facts: &Facts, has_critical: bool, capacity_bytes: u64) -> SuitabilityReport {
    let bad_sectors = facts.pending_or_uncorrectable > 0 || facts.reallocated > 0;

    let os_install = if score >= 80 && !bad_sectors && facts.p99_latency_ok && facts.os_speed_ok {
        suit(SuitLevel::Good, "suitability.os_install.ok", score)
    } else {
        let code = if bad_sectors {
            "suitability.os_install.bad_sectors"
        } else if !facts.os_speed_ok {
            "suitability.os_install.slow_speed"
        } else if !facts.p99_latency_ok {
            "suitability.os_install.slow_latency"
        } else {
            "suitability.os_install.score_low"
        };
        let level = if score >= 65 && !has_critical && facts.pending_or_uncorrectable == 0 {
            SuitLevel::Caution
        } else {
            SuitLevel::No
        };
        suit(level, code, score)
    };

    let long_term_storage = if score >= 75 && !has_critical {
        suit(SuitLevel::Good, "suitability.long_term_storage.ok", score)
    } else if has_critical {
        suit(SuitLevel::No, "suitability.long_term_storage.critical_issues", score)
    } else {
        let level = if score >= 60 { SuitLevel::Caution } else { SuitLevel::No };
        suit(level, "suitability.long_term_storage.score_low", score)
    };

    let daily_use = if score >= 65 && !has_critical {
        suit(SuitLevel::Good, "suitability.daily_use.ok", score)
    } else if score >= 50 && !has_critical {
        suit(SuitLevel::Caution, "suitability.daily_use.caution", score)
    } else {
        suit(SuitLevel::No, "suitability.daily_use.no", score)
    };

    let pct = safe_fill_percent(facts, score, has_critical);
    SuitabilityReport {
        os_install,
        long_term_storage,
        daily_use,
        safe_fill_bytes: (capacity_bytes as u128 * pct as u128 / 100) as u64,
        safe_fill_percent: pct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Metrics;

    fn tr(id: TestId, metrics: &[(&str, Value)], issues: Vec<Issue>) -> TestResult {
        let mut m = Metrics::new();
        for (k, v) in metrics {
            m.insert(k.to_string(), v.clone());
        }
        TestResult::from_issues(id, 0, m, issues)
    }
    fn clean() -> Vec<TestResult> {
        TestId::ALL.iter().map(|i| tr(*i, &[], vec![])).collect()
    }
    fn with(tests: &mut [TestResult], id: TestId, k: &str, v: Value) {
        tests.iter_mut().find(|t| t.id == id).unwrap().metrics.insert(k.into(), v);
    }

    #[test]
    fn clean_is_100_buy() {
        let t = clean();
        assert_eq!(overall_score(&t), 100);
        assert_eq!(verdict(100), Verdict::Buy);
    }

    #[test]
    fn verdict_boundaries() {
        assert_eq!(verdict(80), Verdict::Buy);
        assert_eq!(verdict(79), Verdict::Caution);
        assert_eq!(verdict(50), Verdict::Caution);
        assert_eq!(verdict(49), Verdict::Avoid);
        assert_eq!(verdict(0), Verdict::Avoid);
    }

    #[test]
    fn deductions_by_severity() {
        assert_eq!(deduction(&Issue::info("x")), 0.0);
        assert_eq!(deduction(&Issue::warn("x")), 8.0);
        assert_eq!(deduction(&Issue::critical("x")), 25.0);
        assert!(deduction(&Issue::warn("x").with("count", 50)) > 8.0);
        assert!(deduction(&Issue::critical("x").with("count", 1000)) <= 40.0);
        assert_eq!(test_score(&[Issue::warn("a"), Issue::critical("b")]), 67);
    }

    #[test]
    fn cap_smart_failed() {
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "smart_passed", json!(false));
        assert_eq!(overall_score(&t), 10);
    }

    #[test]
    fn cap_fake_capacity() {
        let mut t = clean();
        with(&mut t, TestId::CapacityCheck, "fake_capacity", json!(true));
        assert_eq!(overall_score(&t), 5);
    }

    #[test]
    fn cap_disconnect() {
        let mut t = clean();
        with(&mut t, TestId::Stability, "device_disconnected", json!(true));
        assert_eq!(overall_score(&t), 25);
    }

    #[test]
    fn cap_pending_and_uncorrectable() {
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "pending_sectors", json!(1));
        assert_eq!(overall_score(&t), 45);
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "uncorrectable_sectors", json!(3));
        assert_eq!(overall_score(&t), 45);
    }

    #[test]
    fn cap_reallocated() {
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "reallocated_sectors", json!(1));
        assert_eq!(overall_score(&t), 70);
        with(&mut t, TestId::SmartHealth, "reallocated_sectors", json!(100));
        assert_eq!(overall_score(&t), 70);
        with(&mut t, TestId::SmartHealth, "reallocated_sectors", json!(101));
        assert_eq!(overall_score(&t), 40);
    }

    #[test]
    fn cap_wear() {
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "wear_used_percent", json!(89.0));
        assert_eq!(overall_score(&t), 100);
        with(&mut t, TestId::SmartHealth, "wear_used_percent", json!(90.0));
        assert_eq!(overall_score(&t), 40);
    }

    #[test]
    fn cap_read_errors() {
        let mut t = clean();
        with(&mut t, TestId::SurfaceRead, "read_errors", json!(2));
        assert_eq!(overall_score(&t), 30);
    }

    #[test]
    fn cap_reasons_are_reported_worst_first() {
        let mut t = clean();
        assert!(cap_reasons(&Facts::from_tests(&t)).is_empty());
        assert_eq!(score_reasons(&Facts::from_tests(&t), 100), Vec::<String>::new());
        assert_eq!(score_reasons(&Facts::from_tests(&t), 80), vec!["score.reason.findings"]);
        with(&mut t, TestId::SmartHealth, "pending_sectors", json!(40));
        with(&mut t, TestId::SmartHealth, "reallocated_sectors", json!(736));
        with(&mut t, TestId::SurfaceRead, "read_errors", json!(1));
        let f = Facts::from_tests(&t);
        assert_eq!(hard_cap(&f), 30);
        assert_eq!(score_reasons(&f, 0), vec!["score.reason.read_errors", "score.reason.many_reallocated", "score.reason.unreadable"]);
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "smart_passed", json!(false));
        with(&mut t, TestId::SmartHealth, "reallocated_sectors", json!(3));
        assert_eq!(score_reasons(&Facts::from_tests(&t), 10), vec!["score.reason.smart_failed", "score.reason.reallocated"]);
    }

    /// The owner's real failing ST2000LM007 (v0.1.0 test): must stay BAD 0/100 / Avoid, with an explanation.
    #[test]
    fn real_failing_hdd_report_is_avoid_zero_with_reasons() {
        use crate::tests::{selftest_short, surface_read};
        let smart = {
            let mut m = Metrics::new();
            for (k, v) in [("smart_passed", json!(true)), ("reallocated_sectors", json!(736)), ("pending_sectors", json!(40)), ("uncorrectable_sectors", json!(40)), ("temperature_c", json!(29))] {
                m.insert(k.into(), v);
            }
            let issues = vec![Issue::critical("issue.smart.reallocated_sectors").with("count", 736), Issue::critical("issue.smart.unreadable_sectors").with("count", 40)];
            TestResult::from_issues(TestId::SmartHealth, 0, m, issues)
        };
        let selftest = selftest_short::finish(90_000, Metrics::new(), crate::smart::SelfTestState::Unknown, true, 90);
        assert_eq!((selftest.status, selftest.score), (Status::Skipped, None));
        // surface: 13 of 576 samples, one 12.6 s stall, one read error, only 4 MiB read sequentially
        let mut lat = vec![120.0, 125.0, 129.2, 130.0, 118.0, 140.0, 122.0, 127.0, 135.0, 115.0, 128.0, 126.0];
        lat.push(12_599.73);
        let (mut m, mut issues) = surface_read::assess(surface_read::DriveClass::Hdd, 1 << 20, &lat, 1, 4_194_304, 0.3);
        issues.extend(surface_read::coverage_issue(13, 576, false, true));
        m.insert("read_errors".into(), json!(1));
        m.insert("samples_total".into(), json!(13));
        m.insert("samples_planned".into(), json!(576));
        let surface = TestResult::from_issues(TestId::SurfaceRead, 0, m, issues);
        assert!(!surface.issues.iter().any(|i| i.code == "issue.surface_read.slow_drive"));
        assert!(surface.issues.iter().any(|i| i.code == "issue.surface_read.partial_coverage"));
        let mut sm = Metrics::new();
        sm.insert("device_disconnected".into(), json!(false));
        let stab_issues = vec![Issue::warn("issue.stability.io_errors").with("count", 1), Issue::critical("issue.stability.new_bad_sectors").with("count", 312)];
        let stab = TestResult::from_issues(TestId::Stability, 0, sm, stab_issues);
        let mut t = vec![tr(TestId::DriveInfo, &[], vec![]), smart, selftest, surface, TestResult::skipped(TestId::WriteVerify, vec![]), TestResult::skipped(TestId::CapacityCheck, vec![]), stab];
        t.sort_by_key(|x| TestId::ALL.iter().position(|i| *i == x.id));
        let sc = overall_score(&t);
        assert_eq!(sc, 0);
        assert_eq!(verdict(sc), Verdict::Avoid);
        let f = Facts::from_tests(&t);
        assert_eq!(suitability(sc, &f, true, 2_000_398_934_016).safe_fill_percent, 0);
        assert_eq!(score_reasons(&f, sc), vec!["score.reason.read_errors", "score.reason.many_reallocated", "score.reason.unreadable"]);
    }

    #[test]
    fn lowest_cap_wins_and_issues_deduct() {
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "smart_passed", json!(false));
        with(&mut t, TestId::CapacityCheck, "fake_capacity", json!(true));
        assert_eq!(overall_score(&t), 5);
        let t2 = vec![tr(TestId::Stability, &[], vec![Issue::critical("a"), Issue::critical("b"), Issue::critical("c"), Issue::critical("d")])];
        assert_eq!(overall_score(&t2), 0);
    }

    #[test]
    fn safe_fill_score_fallback_table() {
        assert_eq!(safe_fill_percent_for_score(100), 90);
        assert_eq!(safe_fill_percent_for_score(80), 90);
        assert_eq!(safe_fill_percent_for_score(79), 60);
        assert_eq!(safe_fill_percent_for_score(65), 60);
        assert_eq!(safe_fill_percent_for_score(64), 30);
        assert_eq!(safe_fill_percent_for_score(50), 30);
        assert_eq!(safe_fill_percent_for_score(49), 0);
    }

    fn facts_ok() -> Facts {
        Facts { p99_latency_ok: true, os_speed_ok: true, ..Default::default() }
    }

    #[test]
    fn safe_fill_follows_real_problems_not_score() {
        let mut f = facts_ok();
        f.capacity_verified = true;
        assert_eq!(safe_fill_percent(&f, 60, false), 100); // many warnings, no real fault
        assert_eq!(safe_fill_percent(&f, 100, false), 100);
        f.capacity_verified = false;
        assert_eq!(safe_fill_percent(&f, 95, false), 90); // capacity unproven (safe mode)
        let mut f = facts_ok();
        f.reallocated = 3;
        assert_eq!(safe_fill_percent(&f, 70, false), 60);
        f.reallocated = 101;
        assert_eq!(safe_fill_percent(&f, 40, false), 0);
        for mutate in [
            (|f: &mut Facts| f.fake_capacity = true) as fn(&mut Facts),
            |f| f.read_errors = 1,
            |f| f.pending_or_uncorrectable = 1,
            |f| f.smart_failed = true,
            |f| f.disconnected = true,
        ] {
            let mut f = facts_ok();
            f.capacity_verified = true;
            mutate(&mut f);
            assert_eq!(safe_fill_percent(&f, 80, true), 0);
        }
        let mut f = facts_ok();
        f.wear_used_percent = Some(95.0);
        assert_eq!(safe_fill_percent(&f, 90, false), 30);
        // unexplained critical issue falls back to the score table
        assert_eq!(safe_fill_percent(&facts_ok(), 55, true), 30);
    }

    #[test]
    fn suitability_good_drive() {
        let s = suitability(95, &facts_ok(), false, 1000);
        assert!(s.os_install.ok && s.long_term_storage.ok && s.daily_use.ok);
        assert_eq!(s.safe_fill_bytes, 900);
        assert_eq!(s.os_install.reason_code, "suitability.os_install.ok");
    }

    #[test]
    fn suitability_os_install_needs_clean_sectors_and_latency() {
        let mut f = facts_ok();
        f.reallocated = 1;
        let s = suitability(85, &f, false, 1000);
        assert!(!s.os_install.ok);
        assert_eq!(s.os_install.reason_code, "suitability.os_install.bad_sectors");
        assert!(s.long_term_storage.ok);

        let mut f = facts_ok();
        f.p99_latency_ok = false;
        let s = suitability(90, &f, false, 1000);
        assert!(!s.os_install.ok);
        assert_eq!(s.os_install.reason_code, "suitability.os_install.slow_latency");

        let s = suitability(79, &facts_ok(), false, 1000);
        assert!(!s.os_install.ok);
        assert_eq!(s.os_install.level, SuitLevel::Caution);
    }

    #[test]
    fn suitability_long_term_thresholds() {
        assert!(suitability(75, &facts_ok(), false, 10).long_term_storage.ok);
        let s = suitability(74, &facts_ok(), false, 10);
        assert!(!s.long_term_storage.ok);
        assert_eq!(s.long_term_storage.level, SuitLevel::Caution);
        let s = suitability(90, &facts_ok(), true, 10);
        assert_eq!(s.long_term_storage.level, SuitLevel::No);
        assert_eq!(s.long_term_storage.reason_code, "suitability.long_term_storage.critical_issues");
    }

    #[test]
    fn suitability_bad_drive() {
        let s = suitability(20, &facts_ok(), true, 1000);
        assert_eq!(s.safe_fill_percent, 0);
        assert_eq!(s.safe_fill_bytes, 0);
        assert_eq!(s.daily_use.level, SuitLevel::No);
        assert_eq!(s.os_install.level, SuitLevel::No);
    }

    #[test]
    fn issues_sorted_critical_first() {
        let t = vec![tr(TestId::DriveInfo, &[], vec![Issue::info("i"), Issue::critical("c")]), tr(TestId::Stability, &[], vec![Issue::warn("w")])];
        let s = sorted_issues(&t);
        assert_eq!(s.iter().map(|i| i.code.as_str()).collect::<Vec<_>>(), vec!["c", "w", "i"]);
    }

    fn surface(c: crate::tests::surface_read::DriveClass, lat: &[f64], errors: u64, mbps: f64) -> TestResult {
        let (mut m, issues) = crate::tests::surface_read::assess(c, 1 << 20, lat, errors, (mbps * 1_048_576.0 * 15.0) as u64, mbps);
        m.insert("read_errors".into(), json!(errors));
        m.insert("read_mbps_avg".into(), json!(mbps));
        TestResult::from_issues(TestId::SurfaceRead, 0, m, issues)
    }

    fn report_for(surface: TestResult, wipe: bool) -> (u8, SuitabilityReport, Vec<Issue>) {
        let mut t = clean();
        t.retain(|x| x.id != TestId::SurfaceRead);
        t.push(surface);
        if !wipe {
            t.iter_mut().find(|x| x.id == TestId::CapacityCheck).unwrap().status = Status::Skipped;
        }
        let sc = overall_score(&t);
        let issues = sorted_issues(&t);
        let has_critical = issues.iter().any(|i| i.severity == Severity::Critical);
        let s = suitability(sc, &Facts::from_tests(&t), has_critical, 16_000_000_000);
        (sc, s, issues)
    }

    #[test]
    fn regression_slow_healthy_usb_stick_is_buy() {
        use crate::tests::surface_read::{tests::real_stick_latencies, DriveClass};
        let lat = real_stick_latencies();
        for wipe in [false, true] {
            let (score, s, issues) = report_for(surface(DriveClass::Flash, &lat, 0, 17.5), wipe);
            assert!(!issues.iter().any(|i| i.severity == Severity::Critical), "{issues:?}");
            assert!(score >= 80, "score {score}");
            assert_eq!(verdict(score), Verdict::Buy);
            assert!(s.long_term_storage.ok);
            assert!(s.daily_use.ok);
            assert!(!s.os_install.ok);
            assert_eq!(s.os_install.reason_code, "suitability.os_install.slow_speed");
            assert_eq!(s.safe_fill_percent, if wipe { 100 } else { 90 });
            assert!(issues.iter().any(|i| i.code == "issue.surface_read.slow_drive" && i.severity == Severity::Info));
        }
    }

    #[test]
    fn regression_exact_reported_metrics_without_new_keys() {
        // the old report's metrics, as stored: no os_speed_ok / drive_class; the stale p99_latency_ok=false
        let mut t = clean();
        for (k, v) in [("read_mbps_avg", json!(17.5)), ("read_errors", json!(0)), ("p99_latency_ok", json!(false)), ("latency_ms_p50", json!(65.55)), ("latency_ms_p99", json!(1905.74))] {
            with(&mut t, TestId::SurfaceRead, k, v);
        }
        let f = Facts::from_tests(&t);
        assert!(!f.os_speed_ok && f.capacity_verified);
        let sc = overall_score(&t);
        let s = suitability(sc, &f, false, 16_000_000_000);
        assert!(sc >= 80 && s.long_term_storage.ok && s.daily_use.ok);
        assert_eq!(s.os_install.reason_code, "suitability.os_install.slow_speed");
        assert_eq!(s.safe_fill_percent, 100);
        assert_eq!(s.safe_fill_bytes, 16_000_000_000);
    }

    #[test]
    fn genuine_outliers_still_flagged_end_to_end() {
        use crate::tests::surface_read::DriveClass;
        let mut lat = vec![60.0; 252];
        lat.extend([2000.0; 10]);
        let (score, s, issues) = report_for(surface(DriveClass::Flash, &lat, 0, 30.0), true);
        assert!(issues.iter().any(|i| i.code == "issue.surface_read.slow_areas" && i.severity == Severity::Warn));
        assert!(!issues.iter().any(|i| i.severity == Severity::Critical));
        assert!((80..100).contains(&score));
        assert!(s.long_term_storage.ok);
        assert_eq!(s.safe_fill_percent, 100);
    }

    #[test]
    fn truly_failing_drive_is_avoid() {
        use crate::tests::surface_read::DriveClass;
        let mut lat = vec![60.0; 150];
        lat.extend([5000.0; 112]);
        let (score, s, issues) = report_for(surface(DriveClass::Flash, &lat, 40, 3.0), true);
        assert!(issues.iter().any(|i| i.code == "issue.surface_read.read_errors"));
        assert!(issues.iter().any(|i| i.code == "issue.surface_read.slow_areas" && i.severity == Severity::Critical));
        assert_eq!(verdict(score), Verdict::Avoid);
        assert_eq!(s.safe_fill_percent, 0);
        assert_eq!(s.long_term_storage.level, SuitLevel::No);
        assert_eq!(s.daily_use.level, SuitLevel::No);
    }
}
