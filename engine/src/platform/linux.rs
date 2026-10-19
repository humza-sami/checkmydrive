//! Linux (and other non-mac unix): lsblk JSON (sysfs fallback), O_DIRECT raw access.

use crate::report::*;
use crate::util::run_with_timeout;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}
fn as_bool(v: &Value) -> Option<bool> {
    v.as_bool().or_else(|| v.as_str().map(|s| s == "1" || s.eq_ignore_ascii_case("true"))).or_else(|| v.as_u64().map(|n| n != 0))
}
fn st(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

fn collect_mounts(v: &Value, out: &mut Vec<String>) {
    if let Some(a) = v.get("mountpoints").and_then(|x| x.as_array()) {
        for m in a.iter().filter_map(|x| x.as_str()) {
            out.push(m.to_string());
        }
    }
    if let Some(m) = st(v, "mountpoint") {
        out.push(m);
    }
    if let Some(ch) = v.get("children").and_then(|x| x.as_array()) {
        for c in ch {
            collect_mounts(c, out);
        }
    }
}

fn is_system_mount(m: &str) -> bool {
    m == "/" || m == "[SWAP]" || ["/boot", "/usr", "/var", "/etc"].iter().any(|p| m == *p || m.starts_with(&format!("{}/", p)))
}

/// Pure parser over `lsblk -J -b` output (unit-tested).
pub fn parse_lsblk(root: &Value) -> Vec<DriveInfo> {
    let mut out = Vec::new();
    let Some(devs) = root.get("blockdevices").and_then(|x| x.as_array()) else { return out };
    for d in devs {
        let ty = st(d, "type");
        let virt_ok = super::allow_virtual();
        if ty.as_deref() != Some("disk") && !(virt_ok && ty.as_deref() == Some("loop")) {
            continue;
        }
        let Some(name) = st(d, "name") else { continue };
        if name.starts_with("zram") || name.starts_with("ram") || name.starts_with("nbd") || (name.starts_with("loop") && !virt_ok) {
            continue;
        }
        let path = st(d, "path").unwrap_or_else(|| format!("/dev/{}", name));
        let tran = st(d, "tran");
        let rota = d.get("rota").and_then(as_bool).unwrap_or(false);
        let mut mounts = Vec::new();
        collect_mounts(d, &mut mounts);
        let is_system = mounts.iter().any(|m| is_system_mount(m));
        mounts.retain(|m| !m.starts_with('['));
        mounts.sort();
        mounts.dedup();
        let (kind, bus) = if name.starts_with("nvme") {
            (DriveKind::Nvme, Some("nvme".to_string()))
        } else if name.starts_with("mmcblk") {
            (DriveKind::Sd, Some("sd".to_string()))
        } else if tran.as_deref() == Some("usb") {
            (DriveKind::Usb, Some("usb".to_string()))
        } else if rota {
            (DriveKind::Hdd, tran.clone())
        } else {
            (DriveKind::Ssd, tran.clone())
        };
        let removable = d.get("rm").and_then(as_bool).unwrap_or(false) || d.get("hotplug").and_then(as_bool).unwrap_or(false);
        out.push(DriveInfo {
            id: path.clone(),
            path,
            model: st(d, "model"),
            serial: st(d, "serial"),
            firmware: st(d, "rev"),
            vendor: st(d, "vendor"),
            capacity_bytes: d.get("size").and_then(as_u64).unwrap_or(0),
            kind,
            bus,
            rotation_rpm: None,
            removable,
            is_system_disk: is_system,
            mount_points: mounts,
            smart_supported: None,
            power_on_hours: None,
            manufacture_date: None,
        });
    }
    out
}

fn sysfs_read(p: &str) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Fallback when lsblk is unavailable: /sys/block + /proc/mounts.
fn list_sysfs() -> Vec<DriveInfo> {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/block") else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let skip: &[&str] = if super::allow_virtual() { &["ram", "zram", "sr", "fd", "dm-", "md", "nbd"] } else { &["loop", "ram", "zram", "sr", "fd", "dm-", "md", "nbd"] };
        if skip.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let base = format!("/sys/block/{}", name);
        let sectors: u64 = sysfs_read(&format!("{}/size", base)).and_then(|s| s.parse().ok()).unwrap_or(0);
        let rota = sysfs_read(&format!("{}/queue/rotational", base)) == Some("1".into());
        let removable = sysfs_read(&format!("{}/removable", base)) == Some("1".into());
        let dev = format!("/dev/{}", name);
        let mut mp = Vec::new();
        for l in mounts.lines() {
            let mut it = l.split_whitespace();
            if let (Some(src), Some(m)) = (it.next(), it.next()) {
                if src.starts_with(&dev) {
                    mp.push(m.replace("\\040", " "));
                }
            }
        }
        let is_system = mp.iter().any(|m| is_system_mount(m));
        let usb = std::fs::canonicalize(&base).map(|p| p.to_string_lossy().contains("/usb")).unwrap_or(false);
        let (kind, bus) = if name.starts_with("nvme") {
            (DriveKind::Nvme, Some("nvme".to_string()))
        } else if name.starts_with("mmcblk") {
            (DriveKind::Sd, Some("sd".to_string()))
        } else if usb {
            (DriveKind::Usb, Some("usb".to_string()))
        } else if rota {
            (DriveKind::Hdd, None)
        } else {
            (DriveKind::Ssd, None)
        };
        out.push(DriveInfo {
            id: dev.clone(),
            path: dev,
            model: sysfs_read(&format!("{}/device/model", base)),
            serial: sysfs_read(&format!("{}/device/serial", base)),
            firmware: sysfs_read(&format!("{}/device/rev", base)).or_else(|| sysfs_read(&format!("{}/device/firmware_rev", base))),
            vendor: sysfs_read(&format!("{}/device/vendor", base)),
            capacity_bytes: sectors * 512,
            kind,
            bus,
            rotation_rpm: None,
            removable,
            is_system_disk: is_system,
            mount_points: mp,
            smart_supported: None,
            power_on_hours: None,
            manufacture_date: None,
        });
    }
    out
}

pub fn list_drives() -> Vec<DriveInfo> {
    let cols = "NAME,PATH,MODEL,SERIAL,SIZE,TYPE,TRAN,ROTA,RM,HOTPLUG,VENDOR,REV";
    for mp_col in ["MOUNTPOINTS", "MOUNTPOINT"] {
        let c = format!("{},{}", cols, mp_col);
        if let Ok(o) = run_with_timeout(Path::new("lsblk"), &["-J", "-b", "-o", &c], Duration::from_secs(15)) {
            if o.code == Some(0) {
                if let Ok(v) = serde_json::from_str::<Value>(&o.stdout) {
                    let d = parse_lsblk(&v);
                    if !d.is_empty() {
                        return d;
                    }
                }
            }
        }
    }
    list_sysfs()
}

pub fn unmount(drive: &DriveInfo) -> Result<(), String> {
    // Unmount deepest paths first.
    let mut mps = drive.mount_points.clone();
    mps.sort_by_key(|m| std::cmp::Reverse(m.len()));
    for m in mps {
        let o = run_with_timeout(Path::new("umount"), &[&m], Duration::from_secs(60)).map_err(|e| e.to_string())?;
        if o.code != Some(0) {
            return Err(o.stderr);
        }
    }
    Ok(())
}

pub fn present(drive: &DriveInfo) -> bool {
    let name = drive.path.trim_start_matches("/dev/");
    Path::new(&drive.path).exists() && Path::new(&format!("/sys/block/{}", name)).exists()
}

pub fn smartctl_target(drive: &DriveInfo) -> String {
    drive.path.clone()
}

pub fn smartctl_dirs() -> Vec<PathBuf> {
    ["/usr/sbin", "/usr/local/sbin", "/sbin", "/usr/bin", "/usr/local/bin"].iter().map(PathBuf::from).collect()
}

pub fn open_raw_file(drive: &DriveInfo, write: bool) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true).write(write);
    let flags = if write { libc::O_DIRECT | libc::O_DSYNC } else { libc::O_DIRECT };
    match o.custom_flags(flags).open(&drive.path) {
        Ok(f) => Ok(f),
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => OpenOptions::new().read(true).write(write).open(&drive.path),
        Err(e) => Err(e),
    }
}

pub fn device_size(f: &File, _drive: &DriveInfo) -> Option<u64> {
    let mut f2 = f.try_clone().ok()?;
    f2.seek(SeekFrom::End(0)).ok()
}

pub fn create_temp_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

pub fn open_temp_read(path: &Path, bypass: bool) -> io::Result<(File, bool)> {
    if bypass {
        match OpenOptions::new().read(true).custom_flags(libc::O_DIRECT).open(path) {
            Ok(f) => return Ok((f, true)),
            Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {}
            Err(e) => return Err(e),
        }
    }
    Ok((OpenOptions::new().read(true).open(path)?, false))
}

pub fn full_sync(f: &File) -> io::Result<()> {
    f.sync_all()
}

pub fn drop_cache(f: &File) {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = f.as_raw_fd();
}

pub fn free_bytes(path: &Path) -> Option<u64> {
    super::unix_common::free_bytes(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_lsblk() {
        let v = json!({"blockdevices":[
          {"name":"nvme0n1","path":"/dev/nvme0n1","model":"Samsung SSD 980","serial":"S1","size":"500107862016","type":"disk","tran":"nvme","rota":false,"rm":false,"hotplug":false,"vendor":null,"rev":null,
           "children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/boot/efi"]},{"name":"nvme0n1p2","type":"part","mountpoints":["/"]}]},
          {"name":"sdb","path":"/dev/sdb","model":"Ultra","serial":null,"size":32010928128u64,"type":"disk","tran":"usb","rota":true,"rm":true,"hotplug":true,"vendor":"SanDisk ",
           "children":[{"name":"sdb1","type":"part","mountpoints":["/media/u/STICK"]}]},
          {"name":"loop0","type":"loop"},
          {"name":"sda","path":"/dev/sda","size":1000204886016u64,"type":"disk","tran":"sata","rota":"1","rm":"0","children":[{"name":"sda1","mountpoints":[null]}]}
        ]});
        let d = parse_lsblk(&v);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].kind, DriveKind::Nvme);
        assert!(d[0].is_system_disk);
        assert_eq!(d[1].kind, DriveKind::Usb);
        assert!(!d[1].is_system_disk);
        assert_eq!(d[1].mount_points, vec!["/media/u/STICK"]);
        assert_eq!(d[1].vendor.as_deref(), Some("SanDisk"));
        assert!(d[1].removable);
        assert_eq!(d[2].kind, DriveKind::Hdd);
        assert_eq!(d[2].capacity_bytes, 1000204886016);
    }
}
