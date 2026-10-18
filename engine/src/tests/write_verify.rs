//! write_verify.
//! safe mode: ONE temp file `.checkmydrive-<rand>.tmp` on a mounted writable volume of the drive.
//! wipe mode: raw device regions (device must already be unmounted by the engine).

use super::*;
use crate::cleanup::TempGuard;
use crate::platform::{self, RawDevice};
use crate::util::{rand_hex, round1, AlignedBuf, SplitMix64};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const CHUNK: usize = 1024 * 1024;
pub const MAX_FILE: u64 = 1024 * MIB;
pub const MIN_FILE: u64 = 8 * MIB;
const WRITE_LIMIT: Duration = Duration::from_secs(14);
const VERIFY_LIMIT: Duration = Duration::from_secs(14);

pub fn temp_file_size(free: u64, max_file: u64) -> u64 {
    let want = (free / 20).min(max_file);
    want / MIB * MIB
}

pub fn temp_name() -> String {
    format!(".checkmydrive-{}.tmp", rand_hex(12))
}

/// Deterministic pattern for chunk `idx` of a run keyed by `seed`.
pub fn fill_pattern(seed: u64, idx: u64, buf: &mut [u8]) {
    SplitMix64(seed ^ idx.wrapping_mul(0x9E37_79B9_7F4A_7C15)).fill(buf);
}

fn is_disk_full(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(28) | Some(112)) || e.kind() == std::io::ErrorKind::StorageFull
}

fn candidate_dirs(mount: &str) -> Vec<PathBuf> {
    let root = PathBuf::from(mount);
    let mut v = vec![root.clone()];
    for extra in ["tmp", "Users/Shared", "private/tmp"] {
        let p = root.join(extra);
        if p.is_dir() {
            v.push(p);
        }
    }
    v
}

pub fn run(ctx: &Ctx) -> TestResult {
    run_with(ctx, MAX_FILE)
}

/// Same as `run` with a custom temp-file size cap (used by tests).
pub fn run_with(ctx: &Ctx, max_file: u64) -> TestResult {
    if ctx.is_wipe() { run_raw(ctx) } else { run_file(ctx, max_file) }
}

struct Outcome {
    bytes_written: u64,
    bytes_verified: u64,
    mismatches: u64,
    write_error: bool,
    write_mbps: f64,
    read_mbps: f64,
    first_mismatch: Option<u64>,
    bypass: bool,
}

fn finish(ctx: &Ctx, t0: Instant, o: Outcome, extra: Metrics, mut issues: Vec<Issue>) -> TestResult {
    let mut m = extra;
    m.insert("bytes_written".into(), mval(o.bytes_written));
    m.insert("bytes_verified".into(), mval(o.bytes_verified));
    m.insert("verify_mismatches".into(), mval(o.mismatches));
    m.insert("write_errors".into(), mval(o.write_error as u64));
    m.insert("write_mbps".into(), mval(round1(o.write_mbps)));
    m.insert("read_mbps".into(), mval(round1(o.read_mbps)));
    m.insert("cache_bypass".into(), mval(o.bypass));
    m.insert("first_mismatch_offset".into(), mval(o.first_mismatch));
    if o.write_error {
        issues.push(Issue::critical("issue.write_verify.write_error"));
    }
    if o.mismatches > 0 {
        issues.push(Issue::critical("issue.write_verify.data_mismatch").with("count", o.mismatches));
    }
    if o.bytes_written > 0 && o.write_mbps < 3.0 {
        issues.push(Issue::warn("issue.write_verify.slow_write").with("mbps", round1(o.write_mbps)));
    }
    ctx.progress(Phase::WriteVerify, 100.0, "progress.write_verify.done", None);
    TestResult::from_issues(TestId::WriteVerify, t0.elapsed().as_millis() as u64, m, issues)
}

fn skipped(code: &str) -> TestResult {
    TestResult::skipped(TestId::WriteVerify, vec![Issue::info(code)])
}

// ------------------------------------------------------------------ safe mode

fn run_file(ctx: &Ctx, max_file: u64) -> TestResult {
    let t0 = Instant::now();
    // Pick the writable mount with the most free space.
    let mut mounts: Vec<(u64, String)> = ctx
        .drive
        .mount_points
        .iter()
        .filter_map(|m| platform::free_bytes(Path::new(m)).map(|f| (f, m.clone())))
        .collect();
    mounts.sort_by_key(|m| std::cmp::Reverse(m.0));
    if mounts.is_empty() {
        return skipped("issue.write_verify.no_writable_volume");
    }
    let (free, _) = mounts[0].clone();
    let size = temp_file_size(free, max_file);
    if size < MIN_FILE {
        return skipped("issue.write_verify.insufficient_space");
    }

    // Create the temp file (create_new: can never clobber an existing file).
    let mut created: Option<(PathBuf, std::fs::File, TempGuard)> = None;
    'outer: for (_, mount) in &mounts {
        for dir in candidate_dirs(mount) {
            let path = dir.join(temp_name());
            // Register only after a successful create_new, so the guard can never
            // delete a file we did not create ourselves.
            match platform::create_temp_file(&path) {
                Ok(f) => {
                    let guard = TempGuard::new(&path);
                    created = Some((path, f, guard));
                    break 'outer;
                }
                Err(_) => continue,
            }
        }
    }
    let Some((path, mut file, guard)) = created else { return skipped("issue.write_verify.no_writable_volume") };

    let seed: u64 = rand::random();
    let n_chunks = size / CHUNK as u64;
    let mut buf = AlignedBuf::new(CHUNK);
    let mut o = Outcome { bytes_written: 0, bytes_verified: 0, mismatches: 0, write_error: false, write_mbps: 0.0, read_mbps: 0.0, first_mismatch: None, bypass: false };
    let mut issues = Vec::new();

    // ---- write
    let w0 = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    for i in 0..n_chunks {
        if w0.elapsed() > WRITE_LIMIT || ctx.stab.is_gone() {
            break;
        }
        fill_pattern(seed, i, &mut buf);
        match file.write_all(&buf) {
            Ok(()) => o.bytes_written += CHUNK as u64,
            Err(e) if is_disk_full(&e) => break,
            Err(e) => {
                ctx.stab.report_io_error(&e);
                o.write_error = true;
                break;
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(250) {
            last_emit = Instant::now();
            ctx.progress(Phase::WriteVerify, i as f64 / n_chunks as f64 * 50.0, "progress.write_verify.writing", params([("mb", mval(o.bytes_written / MIB))]));
        }
    }
    if let Err(e) = platform::full_sync(&file) {
        ctx.stab.report_io_error(&e);
        o.write_error = true;
    }
    platform::drop_cache(&file);
    o.write_mbps = o.bytes_written as f64 / MIB as f64 / w0.elapsed().as_secs_f64().max(0.001);
    drop(file);

    // ---- cache-bypassing read-back + verify
    let mut bypass_wanted = true;
    'verify: loop {
        let (mut rf, bypass) = match platform::open_temp_read(&path, bypass_wanted) {
            Ok(x) => x,
            Err(e) => {
                ctx.stab.report_io_error(&e);
                o.mismatches += 1;
                break;
            }
        };
        o.bypass = bypass;
        let mut exp = AlignedBuf::new(CHUNK);
        let r0 = Instant::now();
        o.bytes_verified = 0;
        o.mismatches = 0;
        o.first_mismatch = None;
        let total = o.bytes_written / CHUNK as u64;
        for i in 0..total {
            if r0.elapsed() > VERIFY_LIMIT || ctx.stab.is_gone() {
                break;
            }
            match read_full(&mut rf, &mut buf) {
                Ok(()) => {
                    fill_pattern(seed, i, &mut exp);
                    if buf[..] != exp[..] {
                        o.mismatches += 1;
                        o.first_mismatch.get_or_insert(i * CHUNK as u64);
                    }
                    o.bytes_verified += CHUNK as u64;
                }
                Err(e) => {
                    if i == 0 && bypass && e.raw_os_error() == Some(22) {
                        // Filesystem rejects direct I/O: fall back to fadvise-based bypass.
                        bypass_wanted = false;
                        continue 'verify;
                    }
                    ctx.stab.report_io_error(&e);
                    o.mismatches += 1;
                    o.first_mismatch.get_or_insert(i * CHUNK as u64);
                    o.bytes_verified += CHUNK as u64;
                }
            }
            if last_emit.elapsed() >= Duration::from_millis(250) {
                last_emit = Instant::now();
                ctx.progress(Phase::WriteVerify, 50.0 + i as f64 / total.max(1) as f64 * 45.0, "progress.write_verify.verifying", params([("mb", mval(o.bytes_verified / MIB))]));
            }
        }
        o.read_mbps = o.bytes_verified as f64 / MIB as f64 / r0.elapsed().as_secs_f64().max(0.001);
        break;
    }

    // ---- cleanup
    ctx.progress(Phase::WriteVerify, 96.0, "progress.write_verify.cleaning", None);
    if !guard.remove() {
        issues.push(Issue::warn("issue.write_verify.cleanup_failed").with("path", path.to_string_lossy().to_string()));
    }
    let mut extra = Metrics::new();
    extra.insert("mode".into(), mval("file"));
    extra.insert("file_target_bytes".into(), mval(size));
    finish(ctx, t0, o, extra, issues)
}

fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<()> {
    use std::io::Read;
    let mut done = 0;
    while done < buf.len() {
        let n = f.read(&mut buf[done..])?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "short read"));
        }
        done += n;
    }
    Ok(())
}

// ------------------------------------------------------------------ wipe mode

fn run_raw(ctx: &Ctx) -> TestResult {
    let t0 = Instant::now();
    let dev = match RawDevice::open(&ctx.drive, true) {
        Ok(d) => d,
        Err(e) => {
            ctx.stab.report_io_error(&e);
            return TestResult::from_issues(TestId::WriteVerify, 0, Metrics::new(), vec![Issue::critical("issue.write_verify.write_error")]);
        }
    };
    let cap = dev.size / MIB * MIB;
    if cap < 64 * MIB {
        return skipped("issue.write_verify.insufficient_space");
    }
    const REGIONS: u64 = 4;
    let region_len = (64 * MIB).min(cap / 16) / MIB * MIB;
    let starts: Vec<u64> = (0..REGIONS).map(|r| ((cap / 8 + r * (cap / 4)) / MIB * MIB).min(cap - region_len)).collect();

    let seed: u64 = rand::random();
    let mut buf = AlignedBuf::new(CHUNK);
    let mut o = Outcome { bytes_written: 0, bytes_verified: 0, mismatches: 0, write_error: false, write_mbps: 0.0, read_mbps: 0.0, first_mismatch: None, bypass: true };
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let chunks_per_region = region_len / CHUNK as u64;
    let total = chunks_per_region * REGIONS;
    let w0 = Instant::now();
    let mut written: Vec<(u64, u64)> = Vec::new(); // (device offset, global idx)
    'w: for (r, start) in starts.iter().enumerate() {
        for c in 0..chunks_per_region {
            if w0.elapsed() > WRITE_LIMIT || ctx.stab.is_gone() {
                break 'w;
            }
            let idx = r as u64 * chunks_per_region + c;
            let off = start + c * CHUNK as u64;
            fill_pattern(seed, idx, &mut buf);
            if let Err(e) = dev.write_at(off, &buf) {
                ctx.stab.report_io_error(&e);
                o.write_error = true;
                break 'w;
            }
            written.push((off, idx));
            o.bytes_written += CHUNK as u64;
            if last_emit.elapsed() >= Duration::from_millis(250) {
                last_emit = Instant::now();
                ctx.progress(Phase::WriteVerify, idx as f64 / total as f64 * 50.0, "progress.write_verify.writing", params([("mb", mval(o.bytes_written / MIB))]));
            }
        }
    }
    if let Err(e) = dev.sync() {
        ctx.stab.report_io_error(&e);
        o.write_error = true;
    }
    o.write_mbps = o.bytes_written as f64 / MIB as f64 / w0.elapsed().as_secs_f64().max(0.001);

    let r0 = Instant::now();
    let mut exp = AlignedBuf::new(CHUNK);
    for (n, (off, idx)) in written.iter().enumerate() {
        if r0.elapsed() > VERIFY_LIMIT || ctx.stab.is_gone() {
            break;
        }
        match dev.read_at(*off, &mut buf) {
            Ok(()) => {
                fill_pattern(seed, *idx, &mut exp);
                if buf[..] != exp[..] {
                    o.mismatches += 1;
                    o.first_mismatch.get_or_insert(*off);
                }
            }
            Err(e) => {
                ctx.stab.report_io_error(&e);
                o.mismatches += 1;
                o.first_mismatch.get_or_insert(*off);
            }
        }
        o.bytes_verified += CHUNK as u64;
        if last_emit.elapsed() >= Duration::from_millis(250) {
            last_emit = Instant::now();
            ctx.progress(Phase::WriteVerify, 50.0 + n as f64 / written.len().max(1) as f64 * 50.0, "progress.write_verify.verifying", params([("mb", mval(o.bytes_verified / MIB))]));
        }
    }
    o.read_mbps = o.bytes_verified as f64 / MIB as f64 / r0.elapsed().as_secs_f64().max(0.001);
    let mut extra = Metrics::new();
    extra.insert("mode".into(), mval("raw"));
    extra.insert("regions".into(), mval(REGIONS));
    finish(ctx, t0, o, extra, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizing() {
        assert_eq!(temp_file_size(100 * 1024 * MIB, MAX_FILE), MAX_FILE);
        assert_eq!(temp_file_size(100 * MIB, MAX_FILE), 5 * MIB);
        assert_eq!(temp_file_size(10 * 1024 * MIB, MAX_FILE), 512 * MIB);
    }
    #[test]
    fn name_format() {
        let n = temp_name();
        assert!(n.starts_with(".checkmydrive-") && n.ends_with(".tmp"));
    }
    #[test]
    fn pattern_stable_and_distinct() {
        let mut a = vec![0u8; 4096];
        let mut b = vec![0u8; 4096];
        fill_pattern(1, 0, &mut a);
        fill_pattern(1, 0, &mut b);
        assert_eq!(a, b);
        fill_pattern(1, 1, &mut b);
        assert_ne!(a, b);
    }
}
