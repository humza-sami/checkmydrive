//! stability: passive observations gathered while the other tests ran
//! (device disappearing, I/O errors, timeouts, SMART counter deltas before/after).

use super::*;
use crate::smart::{self, SmartData};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Debug, Default, PartialEq)]
pub struct Deltas {
    pub crc: u64,
    pub reallocated: u64,
    pub pending: u64,
}

fn delta(a: Option<u64>, b: Option<u64>) -> u64 {
    match (a, b) {
        (Some(a), Some(b)) => b.saturating_sub(a),
        _ => 0,
    }
}

pub fn smart_deltas(before: &SmartData, after: &SmartData) -> Deltas {
    Deltas {
        crc: delta(before.crc_errors, after.crc_errors),
        reallocated: delta(before.reallocated, after.reallocated),
        pending: delta(before.pending, after.pending) + delta(before.uncorrectable, after.uncorrectable),
    }
}

pub fn run(ctx: &Ctx) -> TestResult {
    let t0 = Instant::now();
    ctx.progress(Phase::Stability, 10.0, "progress.stability.checking", None);
    let mut m = Metrics::new();
    let mut issues = Vec::new();

    let mut compared = false;
    let mut d = Deltas::default();
    if let (Some(before), Some(sc)) = (ctx.smart_data(), ctx.smartctl.as_deref()) {
        if !ctx.stab.is_gone() {
            let after = smart::read_smart(Some(sc), &ctx.drive, Duration::from_secs(15));
            if let Ok(after) = after.data {
                d = smart_deltas(before, &after);
                compared = true;
            }
        }
    }
    let events = ctx.stab.disconnect_events.load(Ordering::Relaxed);
    let disconnected = ctx.stab.ever_disconnected.load(Ordering::Relaxed);
    let io_errors = ctx.stab.io_errors.load(Ordering::Relaxed);
    let timeouts = ctx.stab.timeouts.load(Ordering::Relaxed);

    m.insert("device_disconnected".into(), mval(disconnected));
    m.insert("disconnect_events".into(), mval(events));
    m.insert("io_errors".into(), mval(io_errors));
    m.insert("timeouts".into(), mval(timeouts));
    m.insert("smart_compared".into(), mval(compared));
    m.insert("crc_error_delta".into(), mval(d.crc));
    m.insert("reallocated_delta".into(), mval(d.reallocated));
    m.insert("pending_delta".into(), mval(d.pending));

    if disconnected {
        issues.push(Issue::critical("issue.stability.device_disconnected").with("count", events.max(1)));
    }
    if io_errors > 0 {
        issues.push(Issue::warn("issue.stability.io_errors").with("count", io_errors));
    }
    if timeouts > 0 {
        issues.push(Issue::warn("issue.stability.timeouts").with("count", timeouts));
    }
    if d.crc > 0 {
        issues.push(Issue::warn("issue.stability.crc_errors_increased").with("count", d.crc));
    }
    if d.reallocated + d.pending > 0 {
        issues.push(Issue::critical("issue.stability.new_bad_sectors").with("count", d.reallocated + d.pending));
    }
    ctx.progress(Phase::Stability, 100.0, "progress.stability.checking", None);
    TestResult::from_issues(TestId::Stability, t0.elapsed().as_millis() as u64, m, issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deltas() {
        let a = SmartData { crc_errors: Some(3), reallocated: Some(0), pending: Some(0), ..Default::default() };
        let b = SmartData { crc_errors: Some(5), reallocated: Some(2), pending: Some(1), ..Default::default() };
        assert_eq!(smart_deltas(&a, &b), Deltas { crc: 2, reallocated: 2, pending: 1 });
        let c = SmartData::default();
        assert_eq!(smart_deltas(&c, &b), Deltas::default());
    }
}
