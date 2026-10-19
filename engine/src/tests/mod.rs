//! The seven tests plus the shared context they run in.

pub mod capacity_check;
pub mod drive_info;
pub mod selftest_short;
pub mod smart_health;
pub mod stability;
pub mod surface_read;
pub mod write_verify;

use crate::blockdev::{self, BlockDev, DriveBackend};
use crate::platform::RawDevice;
use crate::progress::Emitter;
use crate::report::*;
use crate::smart::{self, SelfTestState, SmartData, SmartReading};
use std::path::PathBuf;
use std::time::Duration;
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
    /// The user asked to stop; every loop treats it like a vanished device (but it is not an error).
    pub cancelled: AtomicBool,
}

impl Stab {
    pub fn report_io_error(&self, e: &std::io::Error) {
        self.io_errors.fetch_add(1, Ordering::Relaxed);
        if matches!(e.kind(), std::io::ErrorKind::TimedOut) {
            self.timeouts.fetch_add(1, Ordering::Relaxed);
        }
        if blockdev::is_gone(e) {
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
    /// True when the tests must stop issuing I/O: the device is missing or the run was cancelled.
    pub fn is_gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst) || self.cancelled.load(Ordering::SeqCst)
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub struct Ctx {
    pub drive: DriveInfo,
    pub mode: Mode,
    pub emit: Arc<Emitter>,
    pub smartctl: Option<PathBuf>,
    pub smart: SmartReading,
    pub stab: Arc<Stab>,
    /// Set when the drive is not reachable through the host OS (supplied by an embedding host). `None` on desktop.
    pub backend: Option<Arc<dyn DriveBackend>>,
}

impl Ctx {
    /// Open the raw block device: through the backend when there is one, else the OS raw device.
    pub fn open_dev(&self, write: bool) -> std::io::Result<Box<dyn BlockDev>> {
        match &self.backend {
            Some(b) => b.open(write),
            None => RawDevice::open(&self.drive, write).map(|d| Box::new(d) as Box<dyn BlockDev>),
        }
    }
    /// Can this context run SMART self-tests at all (smartctl on desktop, the backend otherwise)?
    pub fn selftest_possible(&self) -> bool {
        self.backend.is_some() || self.smartctl.is_some()
    }
    pub fn start_selftest(&self) -> bool {
        match (&self.backend, self.smartctl.as_deref()) {
            (Some(b), _) => b.start_short_selftest(),
            (None, Some(sc)) => smart::start_short_selftest(sc, &self.drive, self.smart.device_type.as_deref()),
            _ => false,
        }
    }
    pub fn poll_selftest(&self) -> Option<SelfTestState> {
        match (&self.backend, self.smartctl.as_deref()) {
            (Some(b), _) => b.poll_selftest(),
            (None, Some(sc)) => smart::poll_selftest(sc, &self.drive, self.smart.device_type.as_deref()),
            _ => None,
        }
    }
    pub fn abort_selftest(&self) {
        match (&self.backend, self.smartctl.as_deref()) {
            (Some(b), _) => b.abort_selftest(),
            (None, Some(sc)) => smart::abort_selftest(sc, &self.drive, self.smart.device_type.as_deref()),
            _ => {}
        }
    }
    /// Fresh SMART reading for the before/after comparison; `None` when no reader exists.
    pub fn reread_smart(&self, budget: Duration) -> Option<SmartReading> {
        match (&self.backend, self.smartctl.as_deref()) {
            (Some(b), _) => Some(b.read_smart(budget)),
            (None, Some(sc)) => Some(smart::read_smart(Some(sc), &self.drive, budget)),
            _ => None,
        }
    }
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
