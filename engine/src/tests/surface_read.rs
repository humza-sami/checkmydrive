//! surface_read: read-only raw-device sampling across the whole capacity + 15 s sequential read.

use super::*;
use crate::platform::RawDevice;
use crate::util::{percentile, round1, round2, AlignedBuf, SECTOR};
use rand::Rng;
use std::time::{Duration, Instant};

pub const SAMPLE_LEN: usize = 1024 * 1024;
pub const EVEN_SAMPLES: usize = 512;
pub const RANDOM_SAMPLES: usize = 64;
pub const SAMPLE_PHASE_MAX: Duration = Duration::from_secs(40);
pub const SEQ_DURATION: Duration = Duration::from_secs(15);
pub const SEQ_CHUNK: usize = 4 * 1024 * 1024;
const MAX_CONSECUTIVE_ERRORS: u32 = 20;

pub fn slow_threshold_ms(d: &DriveInfo) -> f64 {
    if d.is_rotational() { 300.0 } else { 50.0 }
}

/// Offsets for the sampling pass: evenly spaced then random, all 4096-aligned.
pub fn plan_offsets(capacity: u64, len: u64, even: usize, random: usize) -> Vec<u64> {
    let cap = capacity / SECTOR as u64 * SECTOR as u64;
    if cap < len {
        return vec![0];
    }
    let span = cap - len;
    let mut v = Vec::with_capacity(even + random);
    for i in 0..even {
        let off = if even > 1 { (span as u128 * i as u128 / (even as u128 - 1)) as u64 } else { 0 };
        v.push(off / SECTOR as u64 * SECTOR as u64);
    }
    let mut rng = rand::thread_rng();
    for _ in 0..random {
        let off = if span == 0 { 0 } else { rng.gen_range(0..=span) };
        v.push(off / SECTOR as u64 * SECTOR as u64);
    }
    v
}

/// Classify latencies (pure; unit-tested).
pub fn summarize(lat_ms: &[f64], threshold_ms: f64) -> (f64, f64, f64, f64, u64) {
    let mut s = lat_ms.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = if s.is_empty() { 0.0 } else { s.iter().sum::<f64>() / s.len() as f64 };
    let slow = s.iter().filter(|l| **l > threshold_ms).count() as u64;
    (avg, percentile(&s, 0.5), percentile(&s, 0.99), s.last().copied().unwrap_or(0.0), slow)
}

pub fn run(ctx: &Ctx) -> TestResult {
    run_with(ctx, SEQ_DURATION)
}

/// Same as `run` with a custom sequential-pass duration (used by tests).
pub fn run_with(ctx: &Ctx, seq_duration: Duration) -> TestResult {
    let t0 = Instant::now();
    let dev = match RawDevice::open(&ctx.drive, false) {
        Ok(d) => d,
        Err(e) => {
            ctx.stab.report_io_error(&e);
            return TestResult::from_issues(
                TestId::SurfaceRead,
                0,
                Metrics::new(),
                vec![Issue::critical("issue.surface_read.cannot_open").with("count", 1)],
            );
        }
    };
    let cap = dev.size;
    let cap_aligned = cap / SECTOR as u64 * SECTOR as u64;
    let len = (SAMPLE_LEN as u64).min(cap_aligned) as usize;
    let threshold = slow_threshold_ms(&ctx.drive);
    let offsets = plan_offsets(cap_aligned, len as u64, EVEN_SAMPLES, RANDOM_SAMPLES);
    let total = offsets.len();

    let mut buf = AlignedBuf::new(len.max(SECTOR));
    let mut lat: Vec<f64> = Vec::with_capacity(total);
    let mut errors = 0u64;
    let mut consecutive = 0u32;
    let mut first_bad: Vec<u64> = Vec::new();
    let mut aborted_early = false;
    let phase_start = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut attempted = 0usize;

    for off in offsets.iter().copied() {
        if phase_start.elapsed() > SAMPLE_PHASE_MAX {
            break;
        }
        if ctx.stab.is_gone() {
            aborted_early = true;
            break;
        }
        let t = Instant::now();
        let r = dev.read_at(off, &mut buf[..len]);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        attempted += 1;
        match r {
            Ok(()) => {
                consecutive = 0;
                lat.push(ms);
            }
            Err(e) => {
                errors += 1;
                consecutive += 1;
                ctx.stab.report_io_error(&e);
                if first_bad.len() < 10 {
                    first_bad.push(off);
                }
                if consecutive >= MAX_CONSECUTIVE_ERRORS {
                    aborted_early = true;
                    break;
                }
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(250) {
            last_emit = Instant::now();
            let pp = attempted as f64 / total as f64 * 70.0;
            ctx.progress(Phase::SurfaceRead, pp, "progress.surface_read.sampling", params([("done", mval(attempted)), ("total", mval(total))]));
        }
    }

    // ---- sequential pass
    let mut seq_bytes: u64 = 0;
    let mut seq_mbps_avg = 0.0;
    let mut seq_min: Option<f64> = None;
    let mut seq_max: Option<f64> = None;
    if !aborted_early && cap_aligned >= SECTOR as u64 {
        let chunk = SEQ_CHUNK.min(cap_aligned as usize);
        let mut sbuf = AlignedBuf::new(chunk);
        let seq_start = Instant::now();
        let mut win_start = Instant::now();
        let mut win_bytes = 0u64;
        let mut off = 0u64;
        let mut seq_errors = 0u32;
        while seq_start.elapsed() < seq_duration && off + chunk as u64 <= cap_aligned && !ctx.stab.is_gone() {
            match dev.read_at(off, &mut sbuf[..chunk]) {
                Ok(()) => {
                    seq_bytes += chunk as u64;
                    win_bytes += chunk as u64;
                    seq_errors = 0;
                }
                Err(e) => {
                    errors += 1;
                    seq_errors += 1;
                    ctx.stab.report_io_error(&e);
                    if first_bad.len() < 10 {
                        first_bad.push(off);
                    }
                    if seq_errors >= 10 {
                        break;
                    }
                }
            }
            off += chunk as u64;
            if win_start.elapsed() >= Duration::from_secs(1) {
                let mbps = win_bytes as f64 / MIB as f64 / win_start.elapsed().as_secs_f64();
                seq_min = Some(seq_min.map_or(mbps, |m: f64| m.min(mbps)));
                seq_max = Some(seq_max.map_or(mbps, |m: f64| m.max(mbps)));
                win_start = Instant::now();
                win_bytes = 0;
            }
            if last_emit.elapsed() >= Duration::from_millis(250) {
                last_emit = Instant::now();
                let pp = 70.0 + seq_start.elapsed().as_secs_f64() / seq_duration.as_secs_f64() * 30.0;
                let cur = seq_bytes as f64 / MIB as f64 / seq_start.elapsed().as_secs_f64().max(0.001);
                ctx.progress(Phase::SurfaceRead, pp, "progress.surface_read.sequential", params([("mbps", mval(cur.round() as u64))]));
            }
        }
        let el = seq_start.elapsed().as_secs_f64().max(0.001);
        seq_mbps_avg = seq_bytes as f64 / MIB as f64 / el;
    }

    let (avg, p50, p99, max, slow) = summarize(&lat, threshold);
    let mut m = Metrics::new();
    m.insert("samples_total".into(), mval(attempted));
    m.insert("samples_ok".into(), mval(lat.len()));
    m.insert("read_errors".into(), mval(errors));
    m.insert("slow_samples".into(), mval(slow));
    m.insert("slow_threshold_ms".into(), mval(threshold));
    m.insert("latency_ms_avg".into(), mval(round2(avg)));
    m.insert("latency_ms_p50".into(), mval(round2(p50)));
    m.insert("latency_ms_p99".into(), mval(round2(p99)));
    m.insert("latency_ms_max".into(), mval(round2(max)));
    m.insert("p99_latency_ok".into(), mval(p99 <= threshold));
    m.insert("read_mbps_avg".into(), mval(round1(seq_mbps_avg)));
    m.insert("read_mbps_min".into(), mval(seq_min.map(round1)));
    m.insert("read_mbps_max".into(), mval(seq_max.map(round1)));
    m.insert("sequential_bytes".into(), mval(seq_bytes));
    m.insert("aborted_early".into(), mval(aborted_early));
    m.insert("first_bad_offsets".into(), mval(first_bad.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(",")));

    let mut issues = Vec::new();
    if errors > 0 {
        issues.push(Issue::critical("issue.surface_read.read_errors").with("count", errors));
    }
    if !lat.is_empty() {
        let pct = slow as f64 / lat.len() as f64 * 100.0;
        if pct >= 10.0 {
            issues.push(Issue::critical("issue.surface_read.slow_areas").with("count", slow).with("percent", round1(pct)));
        } else if pct >= 1.0 && slow >= 2 {
            issues.push(Issue::warn("issue.surface_read.slow_areas").with("count", slow).with("percent", round1(pct)));
        }
    }
    let min_speed = if ctx.drive.is_rotational() { 20.0 } else { 10.0 };
    if seq_bytes > 0 && seq_mbps_avg < min_speed {
        issues.push(Issue::warn("issue.surface_read.low_speed").with("mbps", round1(seq_mbps_avg)));
    }
    ctx.progress(Phase::SurfaceRead, 100.0, "progress.surface_read.done", None);
    TestResult::from_issues(TestId::SurfaceRead, t0.elapsed().as_millis() as u64, m, issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offsets_are_aligned_and_in_range() {
        let cap = 500_000_000_000u64;
        let v = plan_offsets(cap, 1 << 20, 512, 64);
        assert_eq!(v.len(), 576);
        assert!(v.iter().all(|o| o % 4096 == 0 && o + (1 << 20) <= cap));
        assert_eq!(v[0], 0);
        assert!(v[511] >= cap - (1 << 20) - 4096);
        assert!(v[..512].windows(2).all(|w| w[0] <= w[1]));
    }
    #[test]
    fn tiny_device() {
        assert_eq!(plan_offsets(4096, 4096, 512, 4).iter().filter(|o| **o != 0).count(), 0);
    }
    #[test]
    fn latency_summary() {
        let l: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let (avg, p50, p99, max, slow) = summarize(&l, 90.0);
        assert!((avg - 50.5).abs() < 1e-9);
        assert_eq!(max, 100.0);
        assert!((50.0..=51.0).contains(&p50));
        assert!(p99 >= 99.0);
        assert_eq!(slow, 10);
    }
}
