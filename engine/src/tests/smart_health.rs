//! smart_health: interpret the SMART reading obtained during prepare.

use super::*;
use crate::smart::{Protocol, SmartData, SmartError};
use std::time::Instant;

/// Pure analysis of parsed SMART data -> (metrics, issues). Unit-tested against fixtures.
pub fn analyze(d: &SmartData) -> (Metrics, Vec<Issue>) {
    let mut m = Metrics::new();
    let mut issues = Vec::new();
    let proto = match d.protocol {
        Protocol::Ata => "ata",
        Protocol::Nvme => "nvme",
        Protocol::Scsi => "scsi",
        Protocol::Unknown => "unknown",
    };
    m.insert("protocol".into(), mval(proto));
    m.insert("smart_passed".into(), mval(d.passed));
    m.insert("reallocated_sectors".into(), mval(d.reallocated));
    m.insert("pending_sectors".into(), mval(d.pending));
    m.insert("uncorrectable_sectors".into(), mval(d.uncorrectable));
    m.insert("crc_errors".into(), mval(d.crc_errors));
    m.insert("wear_used_percent".into(), mval(d.wear_used_percent));
    m.insert("available_spare_percent".into(), mval(d.available_spare));
    m.insert("media_errors".into(), mval(d.media_errors));
    m.insert("temperature_c".into(), mval(d.temperature_c));
    m.insert("power_on_hours".into(), mval(d.power_on_hours));
    m.insert("power_cycles".into(), mval(d.power_cycles));
    m.insert("error_log_entries".into(), mval(d.error_log_entries));

    if d.passed == Some(false) {
        issues.push(Issue::critical("issue.smart.overall_failed"));
    }
    if let Some(n) = d.reallocated.filter(|n| *n > 0) {
        let sev = if n > 100 { Severity::Critical } else { Severity::Warn };
        issues.push(Issue::new(sev, "issue.smart.reallocated_sectors").with("count", n));
    }
    let pending = d.pending.filter(|n| *n > 0);
    let uncorrectable = d.uncorrectable.filter(|n| *n > 0);
    match (pending, uncorrectable) {
        // Same figure twice reads as a duplicate: say it once.
        (Some(p), Some(u)) if p == u => issues.push(Issue::critical("issue.smart.unreadable_sectors").with("count", p)),
        (p, u) => {
            if let Some(n) = p {
                issues.push(Issue::critical("issue.smart.pending_sectors").with("count", n));
            }
            if let Some(n) = u {
                issues.push(Issue::critical("issue.smart.uncorrectable_sectors").with("count", n));
            }
        }
    }
    if let Some(n) = d.crc_errors.filter(|n| *n > 0) {
        issues.push(Issue::warn("issue.smart.crc_errors").with("count", n));
    }
    if let Some(w) = d.wear_used_percent {
        if w >= 90.0 {
            issues.push(Issue::critical("issue.smart.ssd_worn_out").with("percent", w.round() as u64));
        } else if w >= 70.0 {
            issues.push(Issue::warn("issue.smart.ssd_worn").with("percent", w.round() as u64));
        }
    }
    if let Some(c) = d.critical_warning.filter(|c| *c != 0) {
        issues.push(Issue::critical("issue.smart.nvme_critical_warning").with("value", c));
    }
    if let Some(n) = d.media_errors.filter(|n| *n > 0) {
        issues.push(Issue::critical("issue.smart.nvme_media_errors").with("count", n));
    }
    if let (Some(spare), Some(thr)) = (d.available_spare, d.spare_threshold) {
        if spare <= thr && spare < 100 {
            issues.push(Issue::critical("issue.smart.spare_low").with("percent", spare));
        } else if spare < 50 {
            issues.push(Issue::warn("issue.smart.spare_low").with("percent", spare));
        }
    }
    if let Some(t) = d.temperature_c.filter(|t| *t >= 60) {
        issues.push(Issue::warn("issue.smart.temperature_high").with("celsius", t));
    }
    // ATA error logs are meaningful; NVMe error-log counters are noisy on many controllers.
    if d.protocol == Protocol::Ata {
        if let Some(n) = d.error_log_entries.filter(|n| *n > 0) {
            issues.push(Issue::warn("issue.smart.error_log_entries").with("count", n));
        }
    }
    (m, issues)
}

/// Issue produced when SMART data could not be obtained at all.
pub fn unavailable_issue(err: &SmartError, drive: &DriveInfo) -> Issue {
    let flashy = matches!(drive.kind, DriveKind::Usb | DriveKind::Sd) && drive.rotation_rpm.is_none();
    match err {
        SmartError::NoSmartctl => Issue::warn("issue.smart.unavailable"),
        SmartError::NeedsAdmin => Issue::warn("issue.smart.needs_admin"),
        SmartError::Timeout => Issue::warn("issue.smart.timeout"),
        SmartError::Unsupported => {
            if flashy {
                Issue::info("issue.smart.usb_bridge_blocked")
            } else if matches!(drive.bus.as_deref(), Some("usb")) {
                Issue::warn("issue.smart.usb_bridge_blocked")
            } else {
                Issue::warn("issue.smart.not_supported")
            }
        }
    }
}

pub fn run(ctx: &Ctx) -> TestResult {
    let t0 = Instant::now();
    ctx.progress(Phase::SmartHealth, 30.0, "progress.smart_health.analyzing", None);
    let res = match &ctx.smart.data {
        Ok(d) => {
            let (mut m, issues) = analyze(d);
            m.insert("device_type".into(), mval(ctx.smart.device_type.clone()));
            TestResult::from_issues(TestId::SmartHealth, t0.elapsed().as_millis() as u64, m, issues)
        }
        Err(e) => {
            let mut r = TestResult::skipped(TestId::SmartHealth, vec![unavailable_issue(e, &ctx.drive)]);
            r.duration_ms = t0.elapsed().as_millis() as u64;
            r
        }
    };
    ctx.progress(Phase::SmartHealth, 100.0, "progress.smart_health.analyzing", None);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(d: &SmartData) -> Vec<String> {
        analyze(d).1.into_iter().map(|i| i.code).collect()
    }

    #[test]
    fn equal_pending_and_uncorrectable_are_merged_into_one_issue() {
        let d = SmartData { pending: Some(40), uncorrectable: Some(40), ..Default::default() };
        let (_, issues) = analyze(&d);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "issue.smart.unreadable_sectors");
        assert_eq!(issues[0].severity, Severity::Critical);
        assert_eq!(issues[0].magnitude(), 40.0);
    }

    #[test]
    fn different_counts_keep_both_issues() {
        let d = SmartData { pending: Some(40), uncorrectable: Some(8), ..Default::default() };
        assert_eq!(codes(&d), ["issue.smart.pending_sectors", "issue.smart.uncorrectable_sectors"]);
        let d = SmartData { pending: Some(5), ..Default::default() };
        assert_eq!(codes(&d), ["issue.smart.pending_sectors"]);
        let d = SmartData { uncorrectable: Some(5), ..Default::default() };
        assert_eq!(codes(&d), ["issue.smart.uncorrectable_sectors"]);
    }
}
