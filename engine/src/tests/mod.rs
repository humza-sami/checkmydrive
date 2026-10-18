//! The seven tests plus the shared context they run in.

pub mod capacity_check;
pub mod drive_info;
pub mod selftest_short;
pub mod smart_health;
pub mod stability;
pub mod surface_read;
pub mod write_verify;

use crate::platform;
use crate::progress::Emitter;
use crate::report::*;
use crate::smart::{SmartData, SmartReading};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Passive observations shared by all tests (feeds the `stability` test).
#[derive(Default)]
pub struct Stab {
    pub io_errors: AtomicU64,
    pub timeouts: AtomicU64,
    pub disconnect_events: AtomicU64,
    pub ever_disconnected: AtomicBool,
    /// True while the device is currently missing.
    pub gone: AtomicBool,
}

impl Stab {
    pub fn report_io_error(&self, e: &std::io::Error) {
        self.io_errors.fetch_add(1, Ordering::Relaxed);
        if matches!(e.kind(), std::io::ErrorKind::TimedOut) {
            self.timeouts.fetch_add(1, Ordering::Relaxed);
        }
        if platform::is_gone_error(e) {
            self.mark_gone();
        }
    }
    pub fn mark_gone(&self) {
        if !self.gone.swap(true, Ordering::SeqCst) {
            self.disconnect_events.fetch_add(1, Ordering::Relaxed);
        }
        self.ever_disconnected.store(true, Ordering::SeqCst);
    }
    pub fn mark_back(&self) {
        self.gone.store(false, Ordering::SeqCst);
    }
    pub fn is_gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }
}

pub struct Ctx {
    pub drive: DriveInfo,
    pub mode: Mode,
    pub emit: Arc<Emitter>,
    pub smartctl: Option<PathBuf>,
    pub smart: SmartReading,
    pub stab: Arc<Stab>,
}

impl Ctx {
    pub fn progress(&self, phase: Phase, pp: f64, code: &str, params: Option<Params>) {
        self.emit.progress(phase, pp, code, params);
    }
    pub fn smart_data(&self) -> Option<&SmartData> {
        self.smart.data.as_ref().ok()
    }
    pub fn is_wipe(&self) -> bool {
        self.mode == Mode::Wipe
    }
}

pub fn params<const N: usize>(kv: [(&str, serde_json::Value); N]) -> Option<Params> {
    let mut p = Params::new();
    for (k, v) in kv {
        p.insert(k.to_string(), v);
    }
    Some(p)
}

pub fn mval(v: impl Into<serde_json::Value>) -> serde_json::Value {
    v.into()
}

pub const MIB: u64 = 1024 * 1024;
