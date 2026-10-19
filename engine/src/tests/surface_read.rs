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
/// A sequential speed is only reported when at least this much was read (8 chunks); with less, one
/// stalled read dominates the average and the figure means nothing.
pub const MIN_SEQ_BYTES: u64 = 32 * MIB;

/// Speed with sensible precision: 1 decimal from 1 MB/s up, 2 decimals below (so 0.27 is never shown as 0).
pub fn round_speed(mbps: f64) -> f64 {
    if mbps >= 1.0 { round1(mbps) } else { round2(mbps) }
}

/// Info issue when the scan could not cover the plan because the drive stalled (pure; unit-tested).
pub fn coverage_issue(attempted: usize, planned: usize, aborted_early: bool, trouble: bool) -> Option<Issue> {
    (attempted < planned && !aborted_early && trouble).then(|| Issue::info("issue.surface_read.partial_coverage").with("checked", attempted as u64).with("planned", planned as u64))
}

/// Share of the planned spots that were actually attempted, in percent (1 decimal).
pub fn coverage_percent(attempted: usize, planned: usize) -> f64 {
    if planned == 0 { 100.0 } else { round1(attempted as f64 / planned as f64 * 100.0) }
}

/// Speed class of the drive; decides every latency / throughput threshold below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveClass {
    /// Spinning disk (seek + rotation latency).
    Hdd,
    /// USB sticks and SD cards (cheap controllers, large natural variance, USB 2.0 class speeds).
    Flash,
    /// SATA / NVMe SSDs and anything unknown.
    Ssd,
}

impl DriveClass {
    pub fn of(d: &DriveInfo) -> DriveClass {
        if d.is_rotational() {
            DriveClass::Hdd
        } else if matches!(d.kind, DriveKind::Usb | DriveKind::Sd) {
            DriveClass::Flash
        } else {
            DriveClass::Ssd
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            DriveClass::Hdd => "hdd",
            DriveClass::Flash => "flash",
            DriveClass::Ssd => "ssd",
        }
    }
    /// Fixed access latency (ms) a healthy drive of this class may need before the first byte.
    fn base_ms(self) -> f64 {
        match self {
            DriveClass::Hdd => 250.0,
            DriveClass::Flash => 150.0,
            DriveClass::Ssd => 30.0,
        }
    }
    /// Slowest sustained throughput (MB/s) we still treat as "normal" when sizing the per-sample floor.
    fn floor_mbps(self) -> f64 {
        match self {
            DriveClass::Hdd => 20.0,
            DriveClass::Flash => 10.0,
            DriveClass::Ssd => 50.0,
        }
    }
    /// A sample is an outlier only when it is also this many times slower than the drive's own median.
    fn outlier_factor(self) -> f64 {
        match self {
            DriveClass::Hdd => 10.0,
            DriveClass::Flash => 10.0,
            DriveClass::Ssd => 8.0,
        }
    }
    /// Absolute p99 allowance (ms) before relative scaling (40 x p50) takes over.
    fn p99_floor_ms(self) -> f64 {
        match self {
            DriveClass::Hdd => 1000.0,
            DriveClass::Flash => 3000.0,
            DriveClass::Ssd => 250.0,
        }
    }
    /// Sequential read (MB/s) below which we report `slow_drive` as info / as warn.
    fn slow_drive_mbps(self) -> (f64, f64) {
        match self {
            DriveClass::Hdd => (40.0, 15.0),
            _ => (20.0, 5.0),
        }
    }
    /// Sequential read (MB/s) below which running an operating system from the drive is not advisable.
    pub fn os_min_mbps(self) -> f64 {
        match self {
            DriveClass::Hdd => 40.0,
            _ => 30.0,
        }
    }
}

/// Reads this slow (ms) are treated as hung regardless of the drive class.
pub const HARD_STALL_MS: f64 = 10_000.0;

/// Per-sample floor: fixed access latency plus the time `len` bytes need at the class' minimum throughput.
/// For 1 MiB: HDD 300 ms, SSD 50 ms, USB flash 250 ms.
pub fn slow_floor_ms(class: DriveClass, len_bytes: usize) -> f64 {
    class.base_ms() + len_bytes as f64 / MIB as f64 / class.floor_mbps() * 1000.0
}

/// A sample is a "slow area" only if it is slower than BOTH the class floor and `factor x p50`
/// of this very drive, so a uniformly slow drive has no outliers.
pub fn slow_threshold_ms(class: DriveClass, len_bytes: usize, p50_ms: f64) -> f64 {
    slow_floor_ms(class, len_bytes).max(class.outlier_factor() * p50_ms)
}

/// p99 is acceptable up to the class allowance or 40 x the drive's own median, whichever is larger.
pub fn p99_ok(class: DriveClass, p50_ms: f64, p99_ms: f64) -> bool {
    p99_ms <= class.p99_floor_ms().max(40.0 * p50_ms)
}

/// Pure analysis of the sampling + sequential results: metrics and issues (unit-tested).
pub fn assess(class: DriveClass, len_bytes: usize, lat_ms: &[f64], errors: u64, seq_bytes: u64, seq_mbps: f64) -> (Metrics, Vec<Issue>) {
    let mut sorted = lat_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = percentile(&sorted, 0.5);
    let threshold = slow_threshold_ms(class, len_bytes, p50);
    let (avg, _, p99, max, slow) = summarize(&sorted, threshold);
    let hard_stalls = sorted.iter().filter(|l| **l >= HARD_STALL_MS).count() as u64;
    let p99_ok = p99_ok(class, p50, p99);
    let measured = seq_bytes >= MIN_SEQ_BYTES;
    let os_speed_ok = !(measured && seq_mbps < class.os_min_mbps());

    let mut m = Metrics::new();
    m.insert("drive_class".into(), mval(class.as_str()));
    m.insert("slow_samples".into(), mval(slow));
    m.insert("slow_threshold_ms".into(), mval(round2(threshold)));
    m.insert("hard_stalls".into(), mval(hard_stalls));
    m.insert("latency_ms_avg".into(), mval(round2(avg)));
    m.insert("latency_ms_p50".into(), mval(round2(p50)));
    m.insert("latency_ms_p99".into(), mval(round2(p99)));
    m.insert("latency_ms_max".into(), mval(round2(max)));
    m.insert("p99_latency_ok".into(), mval(p99_ok));
    m.insert("os_speed_ok".into(), mval(os_speed_ok));

    let mut issues = Vec::new();
    if errors > 0 {
        issues.push(Issue::critical("issue.surface_read.read_errors").with("count", errors));
    }
    if !sorted.is_empty() {
        let pct = slow as f64 / sorted.len() as f64 * 100.0;
        let widespread = pct >= 20.0 && slow >= 5;
        if widespread || hard_stalls >= 3 {
            issues.push(Issue::critical("issue.surface_read.slow_areas").with("count", slow).with("percent", round1(pct)));
        } else if slow >= 3 && pct >= 1.0 {
            issues.push(Issue::warn("issue.surface_read.slow_areas").with("count", slow).with("percent", round1(pct)));
        }
    }
    // A failing drive's stalls and read errors are the real problem; "slow drive" would only be noise.
    if measured && errors == 0 && hard_stalls == 0 {
        let (info_below, warn_below) = class.slow_drive_mbps();
        if seq_mbps < warn_below {
            issues.push(Issue::warn("issue.surface_read.slow_drive").with("mbps", round_speed(seq_mbps)));
        } else if seq_mbps < info_below {
            issues.push(Issue::info("issue.surface_read.slow_drive").with("mbps", round_speed(seq_mbps)));
        }
    }
    (m, issues)
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

/// Total shown in progress: the planned count while the pace fits the sampling budget, otherwise the
/// number that will actually be attempted at the current pace (never below what is already done).
pub fn projected_total(planned: usize, attempted: usize, elapsed: Duration, budget: Duration) -> usize {
    if attempted == 0 || elapsed.is_zero() {
        return planned;
    }
    let per = elapsed.as_secs_f64() / attempted as f64;
    let fit = (budget.as_secs_f64() / per).floor() as usize;
    planned.min(fit.max(attempted))
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
    let class = DriveClass::of(&ctx.drive);
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
            let shown = projected_total(total, attempted, phase_start.elapsed(), SAMPLE_PHASE_MAX);
            let pp = attempted as f64 / shown as f64 * 70.0;
            ctx.progress(Phase::SurfaceRead, pp, "progress.surface_read.sampling", params([("done", mval(attempted)), ("total", mval(shown))]));
        }
    }

    // Final sampling update: the total now equals the report's `samples_total`.
    ctx.progress(Phase::SurfaceRead, 70.0, "progress.surface_read.sampling", params([("done", mval(attempted)), ("total", mval(attempted))]));

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

    let (mut m, issues) = assess(class, len, &lat, errors, seq_bytes, seq_mbps_avg);
    m.insert("samples_total".into(), mval(attempted));
    m.insert("samples_planned".into(), mval(total));
    m.insert("samples_ok".into(), mval(lat.len()));
    m.insert("read_errors".into(), mval(errors));
    let measured = seq_bytes >= MIN_SEQ_BYTES;
    m.insert("read_mbps_avg".into(), mval(measured.then(|| round_speed(seq_mbps_avg))));
    m.insert("read_mbps_min".into(), mval(seq_min.filter(|_| measured).map(round_speed)));
    m.insert("read_mbps_max".into(), mval(seq_max.filter(|_| measured).map(round_speed)));
    m.insert("coverage_percent".into(), mval(coverage_percent(attempted, total)));
    m.insert("sequential_bytes".into(), mval(seq_bytes));
    m.insert("aborted_early".into(), mval(aborted_early));
    let trouble = errors > 0 || m.get("slow_samples").and_then(|v| v.as_u64()).unwrap_or(0) > 0 || m.get("hard_stalls").and_then(|v| v.as_u64()).unwrap_or(0) > 0;
    let mut issues = issues;
    issues.extend(coverage_issue(attempted, total, aborted_early, trouble));
    m.insert("first_bad_offsets".into(), mval(first_bad.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(",")));
    ctx.progress(Phase::SurfaceRead, 100.0, "progress.surface_read.done", None);
    TestResult::from_issues(TestId::SurfaceRead, t0.elapsed().as_millis() as u64, m, issues)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
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
    fn progress_total_tracks_actual_plan() {
        let b = SAMPLE_PHASE_MAX;
        // fast drive: whole plan fits
        assert_eq!(projected_total(576, 10, Duration::from_millis(100), b), 576);
        // 40 s budget at ~151 ms per sample -> 264 fit, as in the real report
        assert_eq!(projected_total(576, 100, Duration::from_millis(15_150), b), 264);
        // never below what is already done, never above the plan
        assert_eq!(projected_total(576, 300, Duration::from_secs(60), b), 300);
        assert_eq!(projected_total(576, 0, Duration::ZERO, b), 576);
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

    /// 262 samples shaped like the real-hardware report: p50 65.55, p99 1905.74, max 2446.14, avg ~150.
    pub(crate) fn real_stick_latencies() -> Vec<f64> {
        let mut v: Vec<f64> = (0..131).map(|i| 55.0 + 10.55 * i as f64 / 130.0).collect();
        v.extend((0..100).map(|i| 65.6 + 65.0 * i as f64 / 99.0));
        v.extend((0..27).map(|i| 700.0 + 40.0 * i as f64));
        v.extend([1905.74, 1905.74, 2200.0, 2446.14]);
        v
    }

    #[test]
    fn real_stick_distribution_matches_report() {
        let l = real_stick_latencies();
        assert_eq!(l.len(), 262);
        let mut s = l.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((percentile(&s, 0.5) - 65.55).abs() < 1.0);
        assert!((1800.0..=2000.0).contains(&percentile(&s, 0.99)));
        assert_eq!(*s.last().unwrap(), 2446.14);
    }

    #[test]
    fn thresholds_per_class_for_one_mib() {
        assert_eq!(slow_floor_ms(DriveClass::Hdd, 1 << 20), 300.0);
        assert_eq!(slow_floor_ms(DriveClass::Ssd, 1 << 20), 50.0);
        assert_eq!(slow_floor_ms(DriveClass::Flash, 1 << 20), 250.0);
        // smaller reads get a lower floor, relative rule still dominates on slow drives
        assert!(slow_floor_ms(DriveClass::Flash, 4096) < 250.0);
        assert_eq!(slow_threshold_ms(DriveClass::Flash, 1 << 20, 65.55), 655.5);
        assert_eq!(slow_threshold_ms(DriveClass::Ssd, 1 << 20, 2.0), 50.0);
    }

    #[test]
    fn uniformly_slow_drive_has_no_slow_areas() {
        // every 1 MiB read takes ~57 ms (17.5 MB/s): the old fixed 50 ms limit flagged 100 % of them.
        let l: Vec<f64> = (0..262).map(|i| 55.0 + (i % 5) as f64).collect();
        let (m, issues) = assess(DriveClass::Flash, 1 << 20, &l, 0, 276_824_064, 17.5);
        assert_eq!(m["slow_samples"], 0);
        assert!(!issues.iter().any(|i| i.code == "issue.surface_read.slow_areas"));
        // even if the drive were classed as an SSD, the relative rule keeps the count at 0
        let (m, _) = assess(DriveClass::Ssd, 1 << 20, &l, 0, 276_824_064, 17.5);
        assert_eq!(m["slow_samples"], 0);
    }

    #[test]
    fn genuine_outliers_are_flagged() {
        let mut l: Vec<f64> = vec![60.0; 252];
        l.extend([2000.0; 10]);
        let (m, issues) = assess(DriveClass::Flash, 1 << 20, &l, 0, 1 << 28, 17.5);
        assert_eq!(m["slow_samples"], 10);
        let sa = issues.iter().find(|i| i.code == "issue.surface_read.slow_areas").expect("slow_areas");
        assert_eq!(sa.severity, Severity::Warn);
    }

    #[test]
    fn widespread_outliers_or_hangs_are_critical() {
        let mut l: Vec<f64> = vec![60.0; 200];
        l.extend([2000.0; 62]); // 23.7 %
        let (_, issues) = assess(DriveClass::Flash, 1 << 20, &l, 0, 1 << 28, 17.5);
        assert_eq!(issues.iter().find(|i| i.code == "issue.surface_read.slow_areas").unwrap().severity, Severity::Critical);
        let mut l: Vec<f64> = vec![60.0; 259];
        l.extend([12_000.0; 3]);
        let (_, issues) = assess(DriveClass::Flash, 1 << 20, &l, 0, 1 << 28, 17.5);
        assert_eq!(issues.iter().find(|i| i.code == "issue.surface_read.slow_areas").unwrap().severity, Severity::Critical);
    }

    #[test]
    fn slow_drive_severity_by_speed() {
        let sev = |c, mbps| assess(c, 1 << 20, &[10.0; 10], 0, 1 << 28, mbps).1.iter().find(|i| i.code == "issue.surface_read.slow_drive").map(|i| i.severity);
        assert_eq!(sev(DriveClass::Flash, 17.5), Some(Severity::Info));
        assert_eq!(sev(DriveClass::Flash, 3.0), Some(Severity::Warn));
        assert_eq!(sev(DriveClass::Flash, 25.0), None);
        assert_eq!(sev(DriveClass::Hdd, 35.0), Some(Severity::Info));
        assert_eq!(sev(DriveClass::Hdd, 90.0), None);
    }

    #[test]
    fn speed_precision_never_rounds_to_zero() {
        assert_eq!(round_speed(0.27), 0.27);
        assert_eq!(round_speed(0.3), 0.3);
        assert_eq!(round_speed(17.46), 17.5);
        assert_eq!(round_speed(0.0), 0.0);
        let (_, issues) = assess(DriveClass::Hdd, 1 << 20, &[10.0; 10], 0, 1 << 28, 0.27);
        let i = issues.iter().find(|i| i.code == "issue.surface_read.slow_drive").unwrap();
        assert_eq!(i.params.as_ref().unwrap()["mbps"], 0.27);
    }

    #[test]
    fn speed_needs_a_meaningful_amount_of_data() {
        // exactly 4 MiB (one chunk) is not enough: no slow_drive, speed counts as OK for the OS rule
        let (m, issues) = assess(DriveClass::Hdd, 1 << 20, &[10.0; 10], 0, 4 << 20, 0.3);
        assert!(!issues.iter().any(|i| i.code == "issue.surface_read.slow_drive"));
        assert_eq!(m["os_speed_ok"], true);
        let (_, issues) = assess(DriveClass::Hdd, 1 << 20, &[10.0; 10], 0, MIN_SEQ_BYTES - 1, 0.3);
        assert!(issues.is_empty());
        let (m, issues) = assess(DriveClass::Hdd, 1 << 20, &[10.0; 10], 0, MIN_SEQ_BYTES, 0.3);
        assert!(issues.iter().any(|i| i.code == "issue.surface_read.slow_drive"));
        assert_eq!(m["os_speed_ok"], false);
    }

    #[test]
    fn no_slow_drive_note_when_errors_or_hard_stalls_exist() {
        let slow = |lat: &[f64], errors| assess(DriveClass::Hdd, 1 << 20, lat, errors, 1 << 28, 0.3).1;
        assert!(slow(&[10.0; 10], 0).iter().any(|i| i.code == "issue.surface_read.slow_drive"));
        let with_err = slow(&[10.0; 10], 1);
        assert!(with_err.iter().any(|i| i.code == "issue.surface_read.read_errors"));
        assert!(!with_err.iter().any(|i| i.code == "issue.surface_read.slow_drive"));
        let mut l = vec![10.0; 12];
        l.push(12_599.73);
        assert!(!slow(&l, 0).iter().any(|i| i.code == "issue.surface_read.slow_drive"));
    }

    #[test]
    fn coverage_is_reported_honestly() {
        assert_eq!(coverage_percent(13, 576), 2.3);
        assert_eq!(coverage_percent(576, 576), 100.0);
        let i = coverage_issue(13, 576, false, true).unwrap();
        assert_eq!((i.severity, i.code.as_str()), (Severity::Info, "issue.surface_read.partial_coverage"));
        let p = i.params.unwrap();
        assert_eq!((&p["checked"], &p["planned"]), (&json!(13), &json!(576)));
        assert!(coverage_issue(576, 576, false, true).is_none());
        assert!(coverage_issue(13, 576, true, true).is_none()); // aborted_early has its own explanation
        assert!(coverage_issue(262, 576, false, false).is_none()); // merely slow, healthy: no scary note
    }

    #[test]
    fn stall_tolerance_for_p99() {
        assert!(p99_ok(DriveClass::Flash, 65.55, 1905.74));
        assert!(!p99_ok(DriveClass::Flash, 65.55, 9000.0));
        assert!(!p99_ok(DriveClass::Ssd, 2.0, 400.0));
        assert!(p99_ok(DriveClass::Hdd, 15.0, 900.0));
    }
}
