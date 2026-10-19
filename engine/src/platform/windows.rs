//! Windows: PowerShell Get-Disk enumeration, \\.\PhysicalDriveN raw access
//! (FILE_FLAG_NO_BUFFERING), DeviceIoControl for size / volume lock+dismount.

use crate::report::*;
use crate::util::run_with_timeout;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Ioctl::{
    FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

/// Volumes we locked + dismounted for wipe mode; kept open until process exit.
static LOCKED_VOLUMES: Mutex<Vec<File>> = Mutex::new(Vec::new());

const PS_SCRIPT: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$pd = @(Get-PhysicalDisk)
@(Get-Disk | ForEach-Object {
  $d = $_
  $p = $pd | Where-Object { $_.DeviceId -eq "$($d.Number)" } | Select-Object -First 1
  [pscustomobject]@{
    Number = [int]$d.Number
    FriendlyName = "$($d.FriendlyName)"
    SerialNumber = "$($d.SerialNumber)"
    Size = [uint64]$d.Size
    BusType = "$($d.BusType)"
    IsSystem = [bool]$d.IsSystem
    IsBoot = [bool]$d.IsBoot
    FirmwareVersion = "$($d.FirmwareVersion)"
    Manufacturer = "$($d.Manufacturer)"
    MediaType = "$($p.MediaType)"
    SpindleSpeed = [uint64]$p.SpindleSpeed
    Letters = @(Get-Partition -DiskNumber $d.Number | Where-Object { $_.DriveLetter } | ForEach-Object { "$($_.DriveLetter):\" })
  }
}) | ConvertTo-Json -Compress -Depth 4
"#;

fn st(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Pure parser over the PowerShell JSON (single object or array). Unit-tested.
pub fn parse_ps(v: &Value) -> Vec<DriveInfo> {
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![v],
        _ => vec![],
    };
    let mut out = Vec::new();
    for d in items {
        let Some(n) = d.get("Number").and_then(|x| x.as_u64()) else { continue };
        let bus_raw = st(d, "BusType").unwrap_or_default();
        let size = d.get("Size").and_then(|x| x.as_u64()).unwrap_or(0);
        if size == 0 || ((bus_raw.contains("Virtual") || bus_raw == "Spaces") && !super::allow_virtual()) {
            continue;
        }
        let media = st(d, "MediaType").unwrap_or_default();
        let rpm = d.get("SpindleSpeed").and_then(|x| x.as_u64()).filter(|r| *r > 0 && *r < 100_000).map(|r| r as u32);
        let (kind, bus) = match bus_raw.as_str() {
            "USB" => (if media == "HDD" { DriveKind::Hdd } else { DriveKind::Usb }, "usb"),
            "SD" | "MMC" => (DriveKind::Sd, "sd"),
            "NVMe" => (DriveKind::Nvme, "nvme"),
            "SATA" | "ATA" | "ATAPI" => (media_kind(&media), "sata"),
            "SAS" | "SCSI" => (media_kind(&media), "sas"),
            other => (media_kind(&media), if other.is_empty() { "unknown" } else { "other" }),
        };
        let mounts: Vec<String> = d
            .get("Letters")
            .map(|l| match l {
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                Value::String(s) => vec![s.clone()],
                _ => vec![],
            })
            .unwrap_or_default();
        let is_system = d.get("IsSystem").and_then(|x| x.as_bool()).unwrap_or(false) || d.get("IsBoot").and_then(|x| x.as_bool()).unwrap_or(false);
        out.push(DriveInfo {
            id: format!("PhysicalDrive{}", n),
            path: format!("\\\\.\\PhysicalDrive{}", n),
            model: st(d, "FriendlyName"),
            serial: st(d, "SerialNumber"),
            firmware: st(d, "FirmwareVersion"),
            vendor: st(d, "Manufacturer"),
            capacity_bytes: size,
            kind,
            bus: Some(bus.to_string()),
            rotation_rpm: rpm,
            removable: matches!(bus_raw.as_str(), "USB" | "SD" | "MMC"),
            is_system_disk: is_system,
            mount_points: mounts,
            smart_supported: None,
            power_on_hours: None,
            manufacture_date: None,
        });
    }
    out
}

fn media_kind(m: &str) -> DriveKind {
    match m {
        "HDD" => DriveKind::Hdd,
        "SSD" | "SCM" => DriveKind::Ssd,
        _ => DriveKind::Unknown,
    }
}

pub fn list_drives() -> Vec<DriveInfo> {
    let o = run_with_timeout(
        Path::new("powershell.exe"),
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PS_SCRIPT],
        Duration::from_secs(40),
    );
    let Ok(o) = o else { return vec![] };
    match serde_json::from_str::<Value>(o.stdout.trim()) {
        Ok(v) => parse_ps(&v),
        Err(_) => vec![],
    }
}

fn ioctl_simple(f: &File, code: u32) -> bool {
    let mut ret: u32 = 0;
    unsafe {
        DeviceIoControl(f.as_raw_handle() as HANDLE, code, std::ptr::null(), 0, std::ptr::null_mut(), 0, &mut ret, std::ptr::null_mut()) != 0
    }
}

/// Lock + dismount every volume of the drive; handles are kept open so Windows
/// will accept raw writes to the physical drive.
pub fn unmount(drive: &DriveInfo) -> Result<(), String> {
    for m in &drive.mount_points {
        let letter = m.chars().next().ok_or("empty mount point")?;
        let vol = format!("\\\\.\\{}:", letter);
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&vol)
            .map_err(|e| format!("{}: {}", vol, e))?;
        if !ioctl_simple(&f, FSCTL_LOCK_VOLUME) {
            return Err(format!("could not lock {}", vol));
        }
        if !ioctl_simple(&f, FSCTL_DISMOUNT_VOLUME) {
            return Err(format!("could not dismount {}", vol));
        }
        LOCKED_VOLUMES.lock().unwrap().push(f);
    }
    Ok(())
}

pub fn present(drive: &DriveInfo) -> bool {
    OpenOptions::new().access_mode(0).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE).open(&drive.path).is_ok()
}

pub fn smartctl_target(drive: &DriveInfo) -> String {
    // smartctl maps /dev/sda.. to \\.\PhysicalDrive0.. on Windows.
    let n: u32 = drive.id.trim_start_matches("PhysicalDrive").parse().unwrap_or(0);
    let mut s = String::new();
    let mut k = n;
    loop {
        s.insert(0, (b'a' + (k % 26) as u8) as char);
        if k < 26 {
            break;
        }
        k = k / 26 - 1;
    }
    format!("/dev/sd{}", s)
}

pub fn smartctl_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files\smartmontools\bin"),
        PathBuf::from(r"C:\Program Files (x86)\smartmontools\bin"),
    ]
}

pub fn open_raw_file(drive: &DriveInfo, write: bool) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true).write(write).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    o.custom_flags(if write { FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH } else { FILE_FLAG_NO_BUFFERING });
    o.open(&drive.path)
}

pub fn device_size(f: &File, _drive: &DriveInfo) -> Option<u64> {
    let mut info = GET_LENGTH_INFORMATION { Length: 0 };
    let mut ret: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            f.as_raw_handle() as HANDLE,
            IOCTL_DISK_GET_LENGTH_INFO,
            std::ptr::null(),
            0,
            &mut info as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            &mut ret,
            std::ptr::null_mut(),
        )
    };
    if ok != 0 && info.Length > 0 {
        Some(info.Length as u64)
    } else {
        None
    }
}

pub fn create_temp_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH)
        .open(path)
}

pub fn open_temp_read(path: &Path, bypass: bool) -> io::Result<(File, bool)> {
    let mut o = OpenOptions::new();
    o.read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    if bypass {
        o.custom_flags(FILE_FLAG_NO_BUFFERING);
    }
    Ok((o.open(path)?, bypass))
}

pub fn full_sync(f: &File) -> io::Result<()> {
    f.sync_all()
}

pub fn drop_cache(_f: &File) {}

pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let w: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
    if ok != 0 {
        Some(avail)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_get_disk_json() {
        let v = json!([
            {"Number":0,"FriendlyName":"Samsung SSD 970","SerialNumber":"S4EV_1 ","Size":500107862016u64,"BusType":"NVMe","IsSystem":true,"IsBoot":true,"FirmwareVersion":"2B2QEXM7","Manufacturer":"","MediaType":"SSD","SpindleSpeed":0,"Letters":["C:\\"]},
            {"Number":2,"FriendlyName":"WD Elements","SerialNumber":"WX1","Size":2000398934016u64,"BusType":"USB","IsSystem":false,"IsBoot":false,"MediaType":"HDD","SpindleSpeed":5400,"Letters":"E:\\"}
        ]);
        let d = parse_ps(&v);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].kind, DriveKind::Nvme);
        assert!(d[0].is_system_disk);
        assert_eq!(d[1].id, "PhysicalDrive2");
        assert_eq!(d[1].kind, DriveKind::Hdd);
        assert_eq!(d[1].rotation_rpm, Some(5400));
        assert_eq!(d[1].mount_points, vec!["E:\\"]);
        assert_eq!(smartctl_target(&d[1]), "/dev/sdc");
    }
}
