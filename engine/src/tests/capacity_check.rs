//! capacity_check (wipe mode only): unique signed blocks spread over the claimed capacity.
//! Drives that lie about size alias (wrap) or drop writes, so read-back signatures differ.

use super::*;
use crate::platform::RawDevice;
use crate::util::{AlignedBuf, SplitMix64, SECTOR};
use rand::seq::SliceRandom;
use std::time::{Duration, Instant};

pub const BLOCKS: usize = 320; // contract: >= 256
const MAGIC: &[u8; 8] = b"CMDRIVE1";
const PHASE_LIMIT: Duration = Duration::from_secs(25);

/// Evenly spread 4096-aligned block offsets, always including the very last block.
pub fn plan_blocks(capacity: u64, n: usize) -> Vec<u64> {
    let cap = capacity / SECTOR as u64 * SECTOR as u64;
    let nblocks = cap / SECTOR as u64;
    if nblocks == 0 {
        return vec![];
    }
    let mut v: Vec<u64> = (0..n as u64).map(|i| (nblocks as u128 * i as u128 / n as u128) as u64 * SECTOR as u64).collect();
    v.push((nblocks - 1) * SECTOR as u64);
    v.sort();
    v.dedup();
    v
}

pub fn make_block(buf: &mut [u8], offset: u64, nonce: u64, index: u64) {
    buf[..8].copy_from_slice(MAGIC);
    buf[8..16].copy_from_slice(&offset.to_le_bytes());
    buf[16..24].copy_from_slice(&nonce.to_le_bytes());
    buf[24..32].copy_from_slice(&index.to_le_bytes());
    SplitMix64(nonce ^ offset.rotate_left(17)).fill(&mut buf[32..]);
}

#[derive(Debug, PartialEq)]
pub enum BlockCheck {
    Ok,
    /// Block holds the signature of a different offset (address aliasing).
    Aliased(u64),
    /// Anything else: zeros, garbage, stale data.
    Bad,
}

pub fn check_block(buf: &[u8], offset: u64, nonce: u64, index: u64) -> BlockCheck {
    let mut exp = vec![0u8; buf.len()];
    make_block(&mut exp, offset, nonce, index);
    if buf == &exp[..] {
        return BlockCheck::Ok;
    }
    if &buf[..8] == MAGIC && buf[16..24] == nonce.to_le_bytes() {
        let other = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        if other != offset {
            return BlockCheck::Aliased(other);
        }
    }
    BlockCheck::Bad
}

/// Decide fake-capacity from results. `results`: (offset, check) for every tested block.
/// Returns Some(estimated_real_bytes) when the drive looks fake.
pub fn judge(results: &[(u64, BlockCheck)], last_offset: u64) -> Option<u64> {
    let aliased: Vec<(u64, u64)> = results
        .iter()
        .filter_map(|(o, c)| if let BlockCheck::Aliased(other) = c { Some((*o, *other)) } else { None })
        .collect();
    if !aliased.is_empty() {
        let est = aliased.iter().map(|(o, other)| o.abs_diff(*other)).filter(|d| *d > 0).min().unwrap_or(0);
        return Some(est);
    }
    // Writes silently dropped beyond the real end: the final block fails.
    let last_failed = results.iter().any(|(o, c)| *o == last_offset && *c != BlockCheck::Ok);
    if last_failed {
        let first_bad = results.iter().filter(|(_, c)| *c != BlockCheck::Ok).map(|(o, _)| *o).min().unwrap_or(0);
        return Some(first_bad);
    }
    None
}

pub fn run(ctx: &Ctx) -> TestResult {
    if !ctx.is_wipe() {
        return TestResult::skipped(TestId::CapacityCheck, vec![Issue::info("issue.capacity_check.needs_wipe_mode")]);
    }
    let t0 = Instant::now();
    let dev = match RawDevice::open(&ctx.drive, true) {
        Ok(d) => d,
        Err(e) => {
            ctx.stab.report_io_error(&e);
            return TestResult::from_issues(TestId::CapacityCheck, 0, Metrics::new(), vec![Issue::critical("issue.capacity_check.io_error").with("count", 1)]);
        }
    };
    let offsets = plan_blocks(dev.size, BLOCKS);
    let nonce: u64 = rand::random();
    let mut buf = AlignedBuf::new(SECTOR);
    let mut io_errors = 0u64;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let total = offsets.len();

    // write pass
    let w0 = Instant::now();
    let mut written = Vec::new();
    for (i, off) in offsets.iter().enumerate() {
        if w0.elapsed() > PHASE_LIMIT || ctx.stab.is_gone() {
            break;
        }
        make_block(&mut buf, *off, nonce, i as u64);
        match dev.write_at(*off, &buf) {
            Ok(()) => written.push((i as u64, *off)),
            Err(e) => {
                io_errors += 1;
                ctx.stab.report_io_error(&e);
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(250) {
            last_emit = Instant::now();
            ctx.progress(Phase::CapacityCheck, i as f64 / total as f64 * 50.0, "progress.capacity_check.writing", params([("done", mval(i)), ("total", mval(total))]));
        }
    }
    if let Err(e) = dev.sync() {
        ctx.stab.report_io_error(&e);
        io_errors += 1;
    }

    // read pass in shuffled order (defeats simple sequential-prefetch tricks)
    let mut order = written.clone();
    order.shuffle(&mut rand::thread_rng());
    let r0 = Instant::now();
    let mut results: Vec<(u64, BlockCheck)> = Vec::new();
    for (n, (idx, off)) in order.iter().enumerate() {
        if r0.elapsed() > PHASE_LIMIT || ctx.stab.is_gone() {
            break;
        }
        match dev.read_at(*off, &mut buf) {
            Ok(()) => results.push((*off, check_block(&buf, *off, nonce, *idx))),
            Err(e) => {
                io_errors += 1;
                ctx.stab.report_io_error(&e);
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(250) {
            last_emit = Instant::now();
            ctx.progress(Phase::CapacityCheck, 50.0 + n as f64 / order.len().max(1) as f64 * 50.0, "progress.capacity_check.verifying", params([("done", mval(n)), ("total", mval(order.len()))]));
        }
    }

    let last_offset = *offsets.last().unwrap_or(&0);
    let mismatches = results.iter().filter(|(_, c)| *c != BlockCheck::Ok).count() as u64;
    let aliased = results.iter().filter(|(_, c)| matches!(c, BlockCheck::Aliased(_))).count() as u64;
    let fake = if mismatches > 0 { judge(&results, last_offset) } else { None };

    let mut m = Metrics::new();
    m.insert("sectors_tested".into(), mval(results.len()));
    m.insert("mismatches".into(), mval(mismatches));
    m.insert("aliased".into(), mval(aliased));
    m.insert("io_errors".into(), mval(io_errors));
    m.insert("claimed_bytes".into(), mval(dev.size));
    m.insert("fake_capacity".into(), mval(fake.is_some()));
    m.insert("estimated_real_bytes".into(), mval(fake));
    let mut issues = Vec::new();
    if let Some(real) = fake {
        issues.push(Issue::critical("issue.capacity_check.fake_capacity").with("claimed_bytes", dev.size).with("real_bytes", real));
    } else if mismatches > 0 {
        issues.push(Issue::critical("issue.capacity_check.data_corruption").with("count", mismatches));
    }
    if io_errors > 0 {
        issues.push(Issue::critical("issue.capacity_check.io_error").with("count", io_errors));
    }
    ctx.progress(Phase::CapacityCheck, 100.0, "progress.capacity_check.done", None);
    TestResult::from_issues(TestId::CapacityCheck, t0.elapsed().as_millis() as u64, m, issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_has_at_least_256_distinct_aligned_blocks_incl_last() {
        let cap = 2_000_000_000_000u64;
        let v = plan_blocks(cap, BLOCKS);
        assert!(v.len() >= 256);
        assert!(v.iter().all(|o| o % 4096 == 0));
        assert_eq!(*v.last().unwrap(), (cap / 4096 - 1) * 4096);
        assert_eq!(v[0], 0);
    }

    /// Simulate a fake drive: claimed 64 blocks-worth of space but only `real` is backed (wraps).
    fn simulate(real_blocks: u64, claimed_blocks: u64) -> Vec<(u64, BlockCheck)> {
        let nonce = 42u64;
        let offsets: Vec<u64> = (0..claimed_blocks).map(|b| b * 4096).collect();
        let mut store: std::collections::HashMap<u64, Vec<u8>> = Default::default();
        for (i, off) in offsets.iter().enumerate() {
            let mut b = vec![0u8; 4096];
            make_block(&mut b, *off, nonce, i as u64);
            store.insert(off % (real_blocks * 4096), b);
        }
        offsets
            .iter()
            .enumerate()
            .map(|(i, off)| {
                let got = &store[&(off % (real_blocks * 4096))];
                (*off, check_block(got, *off, nonce, i as u64))
            })
            .collect()
    }

    #[test]
    fn honest_drive_passes() {
        let r = simulate(64, 64);
        assert!(r.iter().all(|(_, c)| *c == BlockCheck::Ok));
        assert_eq!(judge(&r, 63 * 4096), None);
    }

    #[test]
    fn wrapping_fake_drive_detected_with_real_size_estimate() {
        let r = simulate(16, 64);
        let est = judge(&r, 63 * 4096).expect("fake");
        assert_eq!(est, 16 * 4096);
    }

    #[test]
    fn dropped_writes_at_end_detected() {
        let mut r: Vec<(u64, BlockCheck)> = (0..64u64).map(|b| (b * 4096, BlockCheck::Ok)).collect();
        for item in r.iter_mut().skip(40) {
            item.1 = BlockCheck::Bad;
        }
        assert_eq!(judge(&r, 63 * 4096), Some(40 * 4096));
    }

    #[test]
    fn scattered_corruption_is_not_fake() {
        let mut r: Vec<(u64, BlockCheck)> = (0..64u64).map(|b| (b * 4096, BlockCheck::Ok)).collect();
        r[5].1 = BlockCheck::Bad;
        assert_eq!(judge(&r, 63 * 4096), None);
    }

    #[test]
    fn zero_block_is_bad() {
        let z = vec![0u8; 4096];
        assert_eq!(check_block(&z, 0, 1, 0), BlockCheck::Bad);
    }
}
