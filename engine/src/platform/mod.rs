//! OS abstraction: enumeration, raw device access, unmount, cache-bypassing I/O.
//!
//! Each OS module exposes the same free functions; this module re-exports them and
//! provides the cross-platform `RawDevice` wrapper.

use crate::report::DriveInfo;
use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(unix)]
mod unix_common;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_usb;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(all(unix, not(target_os = "macos")))]
use linux as imp;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;

/// Virtual disks (loop devices, RAM disks, disk images, VHD/VHDX) are hidden by default because
/// they are not real drives. Setting `CHECKMYDRIVE_ALLOW_VIRTUAL=1` lists them too; CI uses this to
/// exercise the full test pipeline on hosted runners that have no physical spare disk.
pub fn allow_virtual() -> bool {
    std::env::var("CHECKMYDRIVE_ALLOW_VIRTUAL").map(|v| !v.is_empty() && v != "0").unwrap_or(false)
}

/// Enumerate physical drives using only unprivileged OS tools (no SMART).
pub fn list_drives() -> Vec<DriveInfo> {
    let mut v = imp::list_drives();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Normalise a user-supplied drive id so `disk4`, `/dev/disk4`, `/dev/rdisk4`,
/// `PhysicalDrive2`, `\\.\PhysicalDrive2` and `2` all compare equal where sensible.
pub fn normalize_id(s: &str) -> String {
    let mut s = s.trim().to_lowercase().replace('\\', "/");
    for p in ["//./", "/dev/"] {
        if let Some(r) = s.strip_prefix(p) {
            s = r.to_string();
        }
    }
    if s.starts_with("rdisk") {
        s.remove(0);
    }
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) && cfg!(windows) {
        s = format!("physicaldrive{}", s);
    }
    s
}

pub fn find_drive(id: &str) -> Option<DriveInfo> {
    let want = normalize_id(id);
    list_drives().into_iter().find(|d| normalize_id(&d.id) == want || normalize_id(&d.path) == want)
}

/// Unmount every volume of the drive so raw writes are possible (wipe mode only).
pub fn unmount(drive: &DriveInfo) -> Result<(), String> {
    imp::unmount(drive)
}

/// Is the drive still attached? (Used by the stability watchdog.)
pub fn present(drive: &DriveInfo) -> bool {
    imp::present(drive)
}

/// Device argument to hand to smartctl.
pub fn smartctl_target(drive: &DriveInfo) -> String {
    imp::smartctl_target(drive)
}

pub fn smartctl_dirs() -> Vec<std::path::PathBuf> {
    imp::smartctl_dirs()
}

pub fn smartctl_exe_name() -> &'static str {
    if cfg!(windows) {
        "smartctl.exe"
    } else {
        "smartctl"
    }
}

/// Create a brand-new temp file (never overwrites) for writing, bypassing the cache where possible.
pub fn create_temp_file(path: &Path) -> io::Result<File> {
    imp::create_temp_file(path)
}

/// Open the temp file for reading with the OS cache bypassed. Returns whether bypass is active.
pub fn open_temp_read(path: &Path, bypass: bool) -> io::Result<(File, bool)> {
    imp::open_temp_read(path, bypass)
}

/// Flush a file all the way to the medium (F_FULLFSYNC on macOS).
pub fn full_sync(file: &File) -> io::Result<()> {
    imp::full_sync(file)
}

/// After a sync, ask the OS to drop cached pages of this file (best effort).
pub fn drop_cache(file: &File) {
    imp::drop_cache(file)
}

pub fn free_bytes(path: &Path) -> Option<u64> {
    imp::free_bytes(path)
}

/// Raw block device handle. Opened read-only unless `write` is requested (wipe mode only).
pub struct RawDevice {
    file: File,
    pub size: u64,
    pub writable: bool,
}

impl RawDevice {
    pub fn open(drive: &DriveInfo, write: bool) -> io::Result<RawDevice> {
        let file = imp::open_raw_file(drive, write)?;
        let size = imp::device_size(&file, drive).unwrap_or(drive.capacity_bytes);
        let size = if size == 0 { drive.capacity_bytes } else { size };
        Ok(RawDevice { file, size, writable: write })
    }

    /// Read exactly `buf.len()` bytes at `offset`. Offset/len/buf must be 4096-aligned.
    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.file.read_exact_at(buf, offset)
        }
        #[cfg(windows)]
        {
            let mut done = 0usize;
            while done < buf.len() {
                let n = self.file.seek_read(&mut buf[done..], offset + done as u64)?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short read"));
                }
                done += n;
            }
            Ok(())
        }
    }

    pub fn write_at(&self, offset: u64, buf: &[u8]) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "device opened read-only"));
        }
        #[cfg(unix)]
        {
            self.file.write_all_at(buf, offset)
        }
        #[cfg(windows)]
        {
            let mut done = 0usize;
            while done < buf.len() {
                let n = self.file.seek_write(&buf[done..], offset + done as u64)?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::WriteZero, "short write"));
                }
                done += n;
            }
            Ok(())
        }
    }

    pub fn sync(&self) -> io::Result<()> {
        imp::full_sync(&self.file)
    }
}

impl crate::blockdev::BlockDev for RawDevice {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        RawDevice::read_at(self, offset, buf)
    }
    fn write_at(&self, offset: u64, buf: &[u8]) -> io::Result<()> {
        RawDevice::write_at(self, offset, buf)
    }
    fn sync(&self) -> io::Result<()> {
        RawDevice::sync(self)
    }
}

/// True for OS errors that mean "the device is gone".
pub fn is_gone_error(e: &io::Error) -> bool {
    #[cfg(unix)]
    {
        matches!(e.raw_os_error(), Some(libc::ENXIO) | Some(libc::ENODEV) | Some(libc::ESHUTDOWN))
    }
    #[cfg(windows)]
    {
        // ERROR_FILE_NOT_FOUND(2), ERROR_DEVICE_NOT_CONNECTED(1167), ERROR_NO_SUCH_DEVICE(433), ERROR_BAD_UNIT(20)
        matches!(e.raw_os_error(), Some(2) | Some(20) | Some(433) | Some(1167))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize() {
        assert_eq!(normalize_id("/dev/rdisk4"), "disk4");
        assert_eq!(normalize_id("disk4"), "disk4");
        assert_eq!(normalize_id("/dev/sdb"), "sdb");
        assert_eq!(normalize_id("\\\\.\\PhysicalDrive2"), "physicaldrive2");
    }
}
