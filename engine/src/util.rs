//! Small shared helpers: aligned buffers, PRNG, subprocess with timeout, time.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SECTOR: usize = 4096;

/// Heap buffer aligned to 4096 bytes (required for O_DIRECT / NO_BUFFERING / raw devices).
pub struct AlignedBuf {
    ptr: *mut u8,
    len: usize,
    layout: Layout,
}

unsafe impl Send for AlignedBuf {}

impl AlignedBuf {
    pub fn new(len: usize) -> Self {
        let len = len.max(SECTOR);
        let layout = Layout::from_size_align(len, SECTOR).expect("layout");
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "allocation failed");
        AlignedBuf { ptr, len, layout }
    }
}
impl std::ops::Deref for AlignedBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}
impl std::ops::DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}
impl Drop for AlignedBuf {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) }
    }
}

/// SplitMix64 – tiny, fast, deterministic; used for reproducible test patterns.
pub struct SplitMix64(pub u64);
impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let v = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
}

pub fn rand_hex(n: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..n).map(|_| format!("{:x}", rng.gen_range(0..16))).collect()
}

pub struct CmdOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Run a program, capturing output, killing it after `timeout`.
pub fn run_with_timeout(program: &Path, args: &[&str], timeout: Duration) -> std::io::Result<CmdOutput> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let t1 = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let t2 = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait()? {
            Some(s) => break Some(s),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    let stdout = String::from_utf8_lossy(&t1.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&t2.join().unwrap_or_default()).into_owned();
    Ok(CmdOutput { code: status.and_then(|s| s.code()), stdout, stderr, timed_out })
}

/// Convenience: run a system tool by name (resolved via PATH) and return stdout on success.
pub fn run_tool(name: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let out = run_with_timeout(Path::new(name), args, timeout).ok()?;
    if out.timed_out || out.code != Some(0) {
        return None;
    }
    Some(out.stdout)
}

pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Current UTC time as ISO 8601 (`2026-10-19T03:21:07Z`).
pub fn iso_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

pub const HOURS_PER_YEAR: f64 = 8766.0;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aligned() {
        let b = AlignedBuf::new(8192);
        assert_eq!(b.as_ptr() as usize % SECTOR, 0);
        assert_eq!(b.len(), 8192);
    }
    #[test]
    fn prng_deterministic() {
        let mut a = [0u8; 37];
        let mut b = [0u8; 37];
        SplitMix64(7).fill(&mut a);
        SplitMix64(7).fill(&mut b);
        assert_eq!(a, b);
        SplitMix64(8).fill(&mut b);
        assert_ne!(a, b);
    }
    #[test]
    fn pct() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&v, 0.5), 3.0);
        assert_eq!(percentile(&v, 1.0), 5.0);
    }
    #[test]
    fn iso_format() {
        let s = iso_now();
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
    }
}
