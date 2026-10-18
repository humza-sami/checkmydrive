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

    let mut issues = Vec::new();
    let mut m = Metrics::new();
    m.insert("duration_s".into(), mval(started.elapsed().as_secs()));
    let state_name = match final_state {
        SelfTestState::Passed => "passed",
        SelfTestState::Failed => "failed",
        SelfTestState::Aborted => "aborted",
        _ => "unknown",
    };
    if timed_out {
        smart::abort_selftest(sc, &ctx.drive, dt);
        issues.push(Issue::info("issue.selftest.timeout").with("seconds", limit.as_secs()));
        m.insert("final_state".into(), mval("timeout"));
    } else {
        m.insert("final_state".into(), mval(state_name));
        match final_state {
            SelfTestState::Failed => issues.push(Issue::critical("issue.selftest.failed")),
            SelfTestState::Aborted => issues.push(Issue::warn("issue.selftest.aborted")),
            _ => {}
        }
    }
    m.insert("device_gone".into(), mval(ctx.stab.gone.load(Ordering::Relaxed)));
    ctx.progress(Phase::SelftestShort, 100.0, "progress.selftest_short.done", None);
    TestResult::from_issues(TestId::SelftestShort, t0.elapsed().as_millis() as u64, m, issues)
}
