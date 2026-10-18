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
}

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
        }
    }
}

/// Lowest applicable hard cap (100 when none apply).
pub fn hard_cap(f: &Facts) -> u8 {
    let mut cap = 100u8;
    if f.smart_failed {
        cap = cap.min(CAP_SMART_FAILED);
    }
    if f.fake_capacity {
        cap = cap.min(CAP_FAKE_CAPACITY);
    }
    if f.disconnected {
        cap = cap.min(CAP_DISCONNECTED);
    }
    if f.pending_or_uncorrectable > 0 {
        cap = cap.min(CAP_PENDING);
    }
    if f.reallocated > 100 {
        cap = cap.min(CAP_REALLOCATED_MANY);
    } else if f.reallocated > 0 {
        cap = cap.min(CAP_REALLOCATED);
    }
    if f.wear_used_percent.map(|w| w >= 90.0).unwrap_or(false) {
        cap = cap.min(CAP_WEAR);
    }
    if f.read_errors > 0 {
        cap = cap.min(CAP_READ_ERRORS);
    }
    cap
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

pub fn safe_fill_percent(score: u8) -> u8 {
    match score {
        80..=100 => 90,
        65..=79 => 60,
        50..=64 => 30,
        _ => 0,
    }
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

    let os_install = if score >= 80 && !bad_sectors && facts.p99_latency_ok {
        suit(SuitLevel::Good, "suitability.os_install.ok", score)
    } else {
        let code = if bad_sectors {
            "suitability.os_install.bad_sectors"
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

    let pct = safe_fill_percent(score);
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
    fn lowest_cap_wins_and_issues_deduct() {
        let mut t = clean();
        with(&mut t, TestId::SmartHealth, "smart_passed", json!(false));
        with(&mut t, TestId::CapacityCheck, "fake_capacity", json!(true));
        assert_eq!(overall_score(&t), 5);
        let t2 = vec![tr(TestId::Stability, &[], vec![Issue::critical("a"), Issue::critical("b"), Issue::critical("c"), Issue::critical("d")])];
        assert_eq!(overall_score(&t2), 0);
    }

    #[test]
    fn safe_fill_table() {
        assert_eq!(safe_fill_percent(100), 90);
        assert_eq!(safe_fill_percent(80), 90);
        assert_eq!(safe_fill_percent(79), 60);
        assert_eq!(safe_fill_percent(65), 60);
        assert_eq!(safe_fill_percent(64), 30);
        assert_eq!(safe_fill_percent(50), 30);
        assert_eq!(safe_fill_percent(49), 0);
    }

    fn facts_ok() -> Facts {
        Facts { p99_latency_ok: true, ..Default::default() }
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
}
