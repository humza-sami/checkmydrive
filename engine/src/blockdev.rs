//! Abstract block device + drive backend, so the seven tests run unchanged on top of either the
//! desktop OS raw-device layer (`platform::RawDevice` + smartctl) or a transport supplied by an embedding host.
//!
//! Desktop keeps its existing code paths: `RawDevice` simply implements [`BlockDev`], and the
//! `smartctl` plumbing stays in `smart.rs`. A [`DriveBackend`] is only supplied by transports that
//! have no OS block device (an embedding host supplies one); desktop leaves `Ctx::backend` as `None`.

use crate::smart::{SelfTestState, SmartReading};
use std::io;
use std::time::Duration;

/// Raw, cache-bypassing block access. Offsets and lengths are multiples of 4096.
pub trait BlockDev: Send + Sync {
    /// Usable size in bytes.
    fn size(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
    fn write_at(&self, offset: u64, buf: &[u8]) -> io::Result<()>;
    /// Flush device caches to the medium.
    fn sync(&self) -> io::Result<()>;
}

/// Everything the test suite needs from a drive that is not reachable through the host OS.
pub trait DriveBackend: Send + Sync {
    /// Open the block device (`write` only in wipe mode).
    fn open(&self, write: bool) -> io::Result<Box<dyn BlockDev>>;
    /// Is the drive still attached? (stability watchdog)
    fn is_present(&self) -> bool;
    /// Read SMART (identity + attributes), best effort within `budget`.
    fn read_smart(&self, budget: Duration) -> SmartReading;
    /// Start the drive's short self-test. False when refused / unsupported.
    fn start_short_selftest(&self) -> bool;
    fn poll_selftest(&self) -> Option<SelfTestState>;
    fn abort_selftest(&self);
}

/// True for I/O errors that mean "the device is gone" (OS raw-device errnos, or a transport that
/// reported `NotConnected`).
pub fn is_gone(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotConnected || crate::platform::is_gone_error(e)
}
