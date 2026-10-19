//! selftest_short: `smartctl -t short`, polled until done (≤ 2.5 min). Runs concurrently
//! with surface_read (it only issues SMART commands; the surface test only reads).

use super::*;
use crate::smart::{self, SelfTestState};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub const MAX_WAIT: Duration = Duration::from_secs(150);
const POLL: Duration = Duration::from_secs(4);

fn skipped(why: &str) -> TestResult {
    TestResult::skipped(TestId::SelftestShort, vec![Issue::info(why)])
}

pub fn run(ctx: &Ctx) -> TestResult {
    let t0 = Instant::now();
    let Some(sc) = ctx.smartctl.as_deref() else { return skipped("issue.selftest.smart_unavailable") };
    let Some(d) = ctx.smart_data() else { return skipped("issue.selftest.smart_unavailable") };
    if !d.selftest_supported {
        return skipped("issue.selftest.not_supported");
    }
    let dt = ctx.smart.device_type.as_deref();
    ctx.progress(Phase::SelftestShort, 0.0, "progress.selftest_short.starting", None);
    if !smart::start_short_selftest(sc, &ctx.drive, dt) {
        return skipped("issue.selftest.could_not_start");
    }
    let est = d.selftest_poll_minutes.map(|m| Duration::from_secs(m as u64 * 60)).unwrap_or(Duration::from_secs(120));
    let limit = (est * 3 / 2).clamp(Duration::from_secs(30), MAX_WAIT);

    let started = Instant::now();
    let mut seen_running = false;
    let mut final_state = SelfTestState::Unknown;
    let mut timed_out = false;
    let mut last_poll = Instant::now() - POLL;
    loop {
        if ctx.stab.is_gone() {
            break;
        }
        let el = started.elapsed();
        if el >= limit {
            timed_out = true;
            break;
        }
        if last_poll.elapsed() >= POLL {
            last_poll = Instant::now();
            if let Some(st) = smart::poll_selftest(sc, &ctx.drive, dt) {
                match st {
                    SelfTestState::Running { .. } => seen_running = true,
                    SelfTestState::Unknown => {}
                    done => {
                        if seen_running || el > Duration::from_secs(8) {
                            final_state = done;
                            break;
                        }
                    }
                }
                if let SelfTestState::Running { percent_remaining: Some(r) } = st {
                    let pp = 100.0 - r as f64;
                    ctx.progress(Phase::SelftestShort, pp.max(el.as_secs_f64() / limit.as_secs_f64() * 100.0), "progress.selftest_short.running", params([("percent", mval(pp as u64))]));
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            }
        }
        let pp = el.as_secs_f64() / limit.as_secs_f64() * 100.0;
        ctx.progress(Phase::SelftestShort, pp.min(99.0), "progress.selftest_short.running", params([("seconds", mval(el.as_secs()))]));
        std::thread::sleep(Duration::from_millis(500));
    }

    if timed_out {
        smart::abort_selftest(sc, &ctx.drive, dt);
    }
    let mut m = Metrics::new();
    m.insert("duration_s".into(), mval(started.elapsed().as_secs()));
    m.insert("device_gone".into(), mval(ctx.stab.gone.load(Ordering::Relaxed)));
    ctx.progress(Phase::SelftestShort, 100.0, "progress.selftest_short.done", None);
    finish(t0.elapsed().as_millis() as u64, m, final_state, timed_out, limit.as_secs())
}

/// Build the result (pure; unit-tested). A time-out is "no answer": status skipped, no score.
pub fn finish(duration_ms: u64, mut m: Metrics, final_state: SelfTestState, timed_out: bool, limit_secs: u64) -> TestResult {
    let (name, issues) = conclude(final_state, timed_out, limit_secs);
    m.insert("final_state".into(), mval(name));
    let mut r = TestResult::from_issues(TestId::SelftestShort, duration_ms, m, issues);
    if timed_out {
        r.status = Status::Skipped;
        r.score = None;
    }
    r
}

/// Map the final self-test state to the metric name and issues (pure; unit-tested).
/// `timed_out` -> a note only (the caller reports the test as skipped); `Unknown` without a time-out means
/// the poll loop ended early (device gone), which is reported as interrupted.
pub fn conclude(final_state: SelfTestState, timed_out: bool, limit_secs: u64) -> (&'static str, Vec<Issue>) {
    if timed_out {
        return ("timeout", vec![Issue::info("issue.selftest.timeout").with("seconds", limit_secs)]);
    }
    match final_state {
        SelfTestState::Passed => ("passed", vec![]),
        SelfTestState::Failed => ("failed", vec![Issue::critical("issue.selftest.failed")]),
        SelfTestState::Aborted => ("aborted", vec![Issue::warn("issue.selftest.aborted")]),
        SelfTestState::Running { .. } | SelfTestState::Unknown => ("interrupted", vec![Issue::warn("issue.selftest.aborted")]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_a_note_not_a_pass() {
        let (name, issues) = conclude(SelfTestState::Unknown, true, 90);
        assert_eq!(name, "timeout");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "issue.selftest.timeout");
        assert_eq!(issues[0].severity, Severity::Info);
        assert_eq!(issues[0].params.as_ref().unwrap()["seconds"], 90);
    }

    #[test]
    fn final_states_map_correctly() {
        assert_eq!(conclude(SelfTestState::Passed, false, 90), ("passed", vec![]));
        let (n, i) = conclude(SelfTestState::Failed, false, 90);
        assert_eq!((n, i[0].severity), ("failed", Severity::Critical));
        let (n, i) = conclude(SelfTestState::Aborted, false, 90);
        assert_eq!((n, i[0].severity, i[0].code.as_str()), ("aborted", Severity::Warn, "issue.selftest.aborted"));
        let (n, i) = conclude(SelfTestState::Unknown, false, 90);
        assert_eq!((n, i[0].severity), ("interrupted", Severity::Warn));
        // a late "passed" report after a time-out must not win
        assert_eq!(conclude(SelfTestState::Passed, true, 90).0, "timeout");
    }

    #[test]
    fn timed_out_result_is_skipped_without_score() {
        let r = finish(90_000, Metrics::new(), SelfTestState::Unknown, true, 90);
        assert_eq!((r.status, r.score), (Status::Skipped, None));
        assert_eq!(r.metrics["final_state"], "timeout");
        assert_eq!(r.issues.len(), 1);
        let ok = finish(1, Metrics::new(), SelfTestState::Passed, false, 90);
        assert_eq!((ok.status, ok.score), (Status::Pass, Some(100)));
        let bad = finish(1, Metrics::new(), SelfTestState::Failed, false, 90);
        assert_eq!(bad.status, Status::Fail);
        let ab = finish(1, Metrics::new(), SelfTestState::Aborted, false, 90);
        assert_eq!(ab.status, Status::Warn);
    }
}
