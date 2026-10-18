//! drive_info: identity sanity checks and age assessment.

use super::*;
use crate::util::{round1, HOURS_PER_YEAR};
use std::time::Instant;

pub const HOURS_OLD: u64 = 35_000; // ~4 years of powered-on time
pub const HOURS_MODERATE: u64 = 20_000; // ~2.3 years

pub fn age_issues(poh: Option<u64>) -> Vec<Issue> {
    match poh {
        Some(h) if h >= HOURS_OLD => vec![Issue::warn("issue.age.high_hours").with("hours", h).with("years", round1(h as f64 / HOURS_PER_YEAR))],
        Some(h) if h >= HOURS_MODERATE => vec![Issue::info("issue.age.moderate_hours").with("hours", h).with("years", round1(h as f64 / HOURS_PER_YEAR))],
        _ => vec![],
    }
}

pub fn run(ctx: &Ctx) -> TestResult {
    let t0 = Instant::now();
    ctx.progress(Phase::DriveInfo, 20.0, "progress.drive_info.identifying", None);
    let d = &ctx.drive;
    let mut issues = Vec::new();
    let mut m = Metrics::new();
    m.insert("model".into(), mval(d.model.clone()));
    m.insert("serial_known".into(), mval(d.serial.is_some()));
    m.insert("capacity_bytes".into(), mval(d.capacity_bytes));
    m.insert("kind".into(), serde_json::to_value(d.kind).unwrap_or_default());
    m.insert("bus".into(), mval(d.bus.clone()));
    m.insert("power_on_hours".into(), mval(d.power_on_hours));
    m.insert("power_on_years".into(), mval(d.power_on_hours.map(|h| round1(h as f64 / HOURS_PER_YEAR))));
    m.insert("is_system_disk".into(), mval(d.is_system_disk));

    if d.capacity_bytes == 0 {
        issues.push(Issue::warn("issue.drive_info.capacity_unknown"));
    }
    if d.model.is_none() {
        issues.push(Issue::info("issue.drive_info.model_unknown"));
    }
    if d.is_system_disk {
        issues.push(Issue::info("issue.drive_info.system_disk"));
    }
    issues.extend(age_issues(d.power_on_hours));
    ctx.progress(Phase::DriveInfo, 100.0, "progress.drive_info.identifying", None);
    TestResult::from_issues(TestId::DriveInfo, t0.elapsed().as_millis() as u64, m, issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn age_thresholds() {
        assert!(age_issues(None).is_empty());
        assert!(age_issues(Some(100)).is_empty());
        assert_eq!(age_issues(Some(25_000))[0].severity, Severity::Info);
        assert_eq!(age_issues(Some(40_000))[0].severity, Severity::Warn);
    }
}
