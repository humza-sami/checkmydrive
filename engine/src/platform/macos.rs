//! macOS: diskutil (plist -> JSON via plutil), /dev/rdiskN raw access, F_NOCACHE.

use crate::report::*;
use super::macos_usb as usb;
use crate::util::run_with_timeout;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn plist_json(cmd: &str) -> Option<Value> {
    let script = format!("{} | /usr/bin/plutil -convert json -o - -", cmd);
    let out = run_with_timeout(Path::new("/bin/sh"), &["-c", &script], Duration::from_secs(20)).ok()?;
    if out.timed_out {
        return None;
    }
    serde_json::from_str(&out.stdout).ok()
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

#[derive(Default, Clone)]
struct Ident {
    serial: Option<String>,
    firmware: Option<String>,
    vendor: Option<String>,
}

fn is_whole_disk(name: &str) -> bool {
    name.strip_prefix("disk").map(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
}

/// Walk `system_profiler -json` output collecting serial/firmware per whole-disk bsd_name.
fn walk_profiler(v: &Value, inh: &Ident, out: &mut HashMap<String, Ident>) {
    match v {
        Value::Object(m) => {
            let mut cur = inh.clone();
            for k in ["device_serial", "serial_num", "serial_number"] {
                if let Some(x) = m.get(k).and_then(|x| x.as_str()) {
                    cur.serial = Some(x.trim().to_string());
                }
            }
            for k in ["device_revision", "bcd_device"] {
                if let Some(x) = m.get(k).and_then(|x| x.as_str()) {
                    cur.firmware = Some(x.trim().to_string());
                }
            }
            if let Some(x) = m.get("manufacturer").and_then(|x| x.as_str()) {
                cur.vendor = Some(x.trim().to_string());
            }
            if let Some(b) = m.get("bsd_name").and_then(|x| x.as_str()) {
                if is_whole_disk(b) && (cur.serial.is_some() || cur.firmware.is_some()) {
                    out.insert(b.to_string(), cur.clone());
                }
            }
            for (k, child) in m {
                // Only children listed under "Media" inherit the parent device identity.
                let pass = if k == "Media" { cur.clone() } else { Ident::default() };
                walk_profiler(child, &pass, out);
            }
        }
        Value::Array(a) => {
            for c in a {
                walk_profiler(c, inh, out);
            }
        }
        _ => {}
    }
}

fn collect_mounts(v: &Value, out: &mut Vec<String>) {
    if let Some(m) = s(v, "MountPoint") {
        out.push(m);
    }
    if let Some(a) = v.get("MountedSnapshots").and_then(|x| x.as_array()) {
        for sn in a {
            if let Some(m) = s(sn, "SnapshotMountPoint") {
                out.push(m);
            }
        }
    }
}

/// Pure parser (unit-tested): builds DriveInfo from the three JSON sources.
pub fn parse_drives(list: &Value, infos: &HashMap<String, Value>, profiler: &Value) -> Vec<DriveInfo> {
    let mut idents = HashMap::new();
    walk_profiler(profiler, &Ident::default(), &mut idents);

    let entries = list.get("AllDisksAndPartitions").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    // physical store partition id -> mount points of volumes in the APFS container
    let mut store_mounts: HashMap<String, Vec<String>> = HashMap::new();
    for e in &entries {
        if let Some(stores) = e.get("APFSPhysicalStores").and_then(|x| x.as_array()) {
            let mut mounts = Vec::new();
            if let Some(vols) = e.get("APFSVolumes").and_then(|x| x.as_array()) {
                for v in vols {
                    collect_mounts(v, &mut mounts);
                }
            }
            for st in stores {
                if let Some(id) = s(st, "DeviceIdentifier") {
                    store_mounts.entry(id).or_default().extend(mounts.clone());
                }
            }
        }
    }

    let mut out = Vec::new();
    for e in &entries {
        if e.get("APFSPhysicalStores").is_some() {
            continue; // synthesized APFS container
        }
        let Some(id) = s(e, "DeviceIdentifier") else { continue };
        if !is_whole_disk(&id) {
            continue;
        }
        let Some(info) = infos.get(&id) else { continue };
        if (s(info, "BusProtocol").as_deref() == Some("Disk Image") || s(info, "VirtualOrPhysical").as_deref() == Some("Virtual")) && !super::allow_virtual() {
            continue;
        }
        let mut mounts: Vec<String> = Vec::new();
        collect_mounts(e, &mut mounts);
        if let Some(parts) = e.get("Partitions").and_then(|x| x.as_array()) {
            for p in parts {
                collect_mounts(p, &mut mounts);
                if let Some(pid) = s(p, "DeviceIdentifier") {
                    if let Some(m) = store_mounts.get(&pid) {
                        mounts.extend(m.clone());
                    }
                }
            }
        }
        mounts.sort();
        mounts.dedup();
        let is_system = mounts.iter().any(|m| m == "/" || m.starts_with("/System/Volumes"));
        // Hide OS-internal mounts of the system disk from the list; they are not user volumes.
        let user_mounts: Vec<String> = mounts.iter().filter(|m| !m.starts_with("/System/Volumes") || *m == "/System/Volumes/Data").cloned().collect();

        let bus_raw = s(info, "BusProtocol");
        let media = s(info, "MediaName");
        let bus = bus_raw.as_ref().map(|b| match b.as_str() {
            "USB" => "usb".to_string(),
            "SATA" => "sata".to_string(),
            "PCI-Express" | "NVMe" | "Apple Fabric" => "nvme".to_string(),
            "Secure Digital" => "sd".to_string(),
            "SAS" => "sas".to_string(),
            other => other.to_lowercase(),
        });
        let solid = info.get("SolidState").and_then(|x| x.as_bool());
        let looks_sd = media.as_deref().map(|m| {
            let l = m.to_lowercase();
            l.contains("sd card") || l.contains("sdxc") || l.contains("sdhc") || l.contains("card reader") || l.contains("microsd")
        }).unwrap_or(false);
        let kind = if bus.as_deref() == Some("sd") || looks_sd {
            DriveKind::Sd
        } else if bus.as_deref() == Some("usb") {
            if solid == Some(false) { DriveKind::Hdd } else { DriveKind::Usb }
        } else if bus.as_deref() == Some("nvme") {
            DriveKind::Nvme
        } else {
            match solid {
                Some(true) => DriveKind::Ssd,
                Some(false) => DriveKind::Hdd,
                None => DriveKind::Unknown,
            }
        };
        let capacity = info.get("TotalSize").or_else(|| info.get("Size")).and_then(|x| x.as_u64())
            .or_else(|| e.get("Size").and_then(|x| x.as_u64())).unwrap_or(0);
        let internal = info.get("Internal").and_then(|x| x.as_bool()).unwrap_or(true);
        let removable = info.get("RemovableMedia").and_then(|x| x.as_bool()).unwrap_or(false)
            || info.get("Removable").and_then(|x| x.as_bool()).unwrap_or(false)
            || !internal;
        let smart_supported = match s(info, "SMARTStatus").as_deref() {
            Some("Not Supported") => Some(false),
            Some(_) => Some(true),
            None => None,
        };
        let poh = info
            .get("SMARTDeviceSpecificKeysMayVaryNotGuaranteed")
            .and_then(|m| m.get("POWER_ON_HOURS_0"))
            .and_then(|x| x.as_u64());
        let ident = idents.get(&id).cloned().unwrap_or_default();
        out.push(DriveInfo {
            id: id.clone(),
            path: format!("/dev/r{}", id),
            model: media,
            serial: ident.serial,
            firmware: ident.firmware,
            vendor: ident.vendor,
            capacity_bytes: capacity,
            kind,
            bus,
            rotation_rpm: None,
            removable,
            is_system_disk: is_system,
            mount_points: user_mounts,
            smart_supported,
            power_on_hours: poh,
            manufacture_date: None,
        });
    }
    out
}

pub fn list_drives() -> Vec<DriveInfo> {
    let Some(list) = plist_json("/usr/sbin/diskutil list -plist") else { return vec![] };
    let mut infos = HashMap::new();
    if let Some(entries) = list.get("AllDisksAndPartitions").and_then(|x| x.as_array()) {
        let ids: Vec<String> = entries.iter().filter(|e| e.get("APFSPhysicalStores").is_none()).filter_map(|e| s(e, "DeviceIdentifier")).collect();
        let handles: Vec<_> = ids
            .into_iter()
            .map(|id| std::thread::spawn(move || (id.clone(), plist_json(&format!("/usr/sbin/diskutil info -plist {}", id)))))
            .collect();
        for h in handles {
            if let Ok((id, Some(v))) = h.join() {
                infos.insert(id, v);
            }
        }
    }
    let profiler = run_with_timeout(
        Path::new("/usr/sbin/system_profiler"),
        &["SPNVMeDataType", "SPUSBDataType", "SPSerialATADataType", "SPStorageDataType", "-json"],
        Duration::from_secs(20),
    )
    .ok()
    .and_then(|o| serde_json::from_str::<Value>(&o.stdout).ok())
    .unwrap_or(Value::Null);
    let mut drives = parse_drives(&list, &infos, &profiler);
    fill_usb_identity(&mut drives, &usb::read_idents);
    drives
}

/// IORegistry fallback: fill missing vendor / serial / firmware of USB drives (never overwrites).
/// `read` is only called when some USB drive actually has a gap.
fn fill_usb_identity(drives: &mut [DriveInfo], read: &dyn Fn() -> HashMap<String, usb::UsbIdent>) {
    let gap = |d: &DriveInfo| d.bus.as_deref() == Some("usb") && (d.serial.is_none() || d.vendor.is_none() || d.firmware.is_none());
    if !drives.iter().any(gap) {
        return;
    }
    let idents = read();
    for d in drives.iter_mut().filter(|d| gap(d)) {
        if let Some(i) = idents.get(&d.id) {
            d.serial = d.serial.take().or_else(|| i.serial.clone());
            d.vendor = d.vendor.take().or_else(|| i.vendor.clone());
            d.firmware = d.firmware.take().or_else(|| i.firmware.clone());
        }
    }
}

pub fn unmount(drive: &DriveInfo) -> Result<(), String> {
    let dev = format!("/dev/{}", drive.id);
    let out = run_with_timeout(Path::new("/usr/sbin/diskutil"), &["unmountDisk", "force", &dev], Duration::from_secs(60)).map_err(|e| e.to_string())?;
    if out.code == Some(0) { Ok(()) } else { Err(out.stderr) }
}

pub fn present(drive: &DriveInfo) -> bool {
    run_with_timeout(Path::new("/usr/sbin/diskutil"), &["info", "-plist", &drive.id], Duration::from_secs(10))
        .map(|o| o.code == Some(0))
        .unwrap_or(true) // if we cannot tell, assume present
}

pub fn smartctl_target(drive: &DriveInfo) -> String {
    format!("/dev/{}", drive.id)
}

pub fn smartctl_dirs() -> Vec<PathBuf> {
    ["/usr/sbin", "/usr/local/sbin", "/opt/homebrew/sbin", "/opt/homebrew/bin", "/usr/local/bin"].iter().map(PathBuf::from).collect()
}

fn no_cache(f: &File) {
    unsafe {
        libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, 1);
    }
}

pub fn open_raw_file(drive: &DriveInfo, write: bool) -> io::Result<File> {
    let f = OpenOptions::new().read(true).write(write).open(&drive.path)?;
    no_cache(&f);
    Ok(f)
}

pub fn device_size(f: &File, _drive: &DriveInfo) -> Option<u64> {
    const DKIOCGETBLOCKSIZE: libc::c_ulong = 0x4004_6418;
    const DKIOCGETBLOCKCOUNT: libc::c_ulong = 0x4008_6419;
    let mut bs: u32 = 0;
    let mut bc: u64 = 0;
    unsafe {
        if libc::ioctl(f.as_raw_fd(), DKIOCGETBLOCKSIZE, &mut bs as *mut u32) != 0 {
            return None;
        }
        if libc::ioctl(f.as_raw_fd(), DKIOCGETBLOCKCOUNT, &mut bc as *mut u64) != 0 {
            return None;
        }
    }
    Some(bs as u64 * bc)
}

pub fn create_temp_file(path: &Path) -> io::Result<File> {
    let f = OpenOptions::new().write(true).create_new(true).open(path)?;
    no_cache(&f);
    Ok(f)
}

pub fn open_temp_read(path: &Path, bypass: bool) -> io::Result<(File, bool)> {
    let f = OpenOptions::new().read(true).open(path)?;
    if bypass {
        no_cache(&f);
    }
    Ok((f, bypass))
}

pub fn full_sync(f: &File) -> io::Result<()> {
    unsafe {
        if libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) == 0 {
            return Ok(());
        }
    }
    f.sync_all()
}

pub fn drop_cache(_f: &File) {}

pub fn free_bytes(path: &Path) -> Option<u64> {
    super::unix_common::free_bytes(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_internal_apple_ssd_and_external_usb() {
        let list = json!({"AllDisksAndPartitions":[
            {"DeviceIdentifier":"disk0","Size":500277792768u64,"Content":"GUID_partition_scheme","Partitions":[
                {"DeviceIdentifier":"disk0s2","Content":"Apple_APFS","Size":494384795648u64}]},
            {"DeviceIdentifier":"disk3","APFSPhysicalStores":[{"DeviceIdentifier":"disk0s2"}],"APFSVolumes":[
                {"DeviceIdentifier":"disk3s5","MountPoint":"/System/Volumes/Data"},
                {"DeviceIdentifier":"disk3s1","MountedSnapshots":[{"SnapshotMountPoint":"/"}]}]},
            {"DeviceIdentifier":"disk4","Size":64000000000u64,"Content":"GUID_partition_scheme","Partitions":[
                {"DeviceIdentifier":"disk4s1","MountPoint":"/Volumes/STICK","Content":"Microsoft Basic Data"}]},
            {"DeviceIdentifier":"disk5","Size":1000u64,"Partitions":[]}
        ]});
        let mut infos = HashMap::new();
        infos.insert("disk0".to_string(), json!({"BusProtocol":"Apple Fabric","SolidState":true,"Internal":true,"MediaName":"APPLE SSD AP0512Z","TotalSize":500277792768u64,"SMARTStatus":"Verified","SMARTDeviceSpecificKeysMayVaryNotGuaranteed":{"POWER_ON_HOURS_0":529}}));
        infos.insert("disk4".to_string(), json!({"BusProtocol":"USB","Internal":false,"RemovableMedia":true,"MediaName":"SanDisk Ultra","TotalSize":64000000000u64,"SMARTStatus":"Not Supported"}));
        infos.insert("disk5".to_string(), json!({"BusProtocol":"Disk Image","TotalSize":1000u64}));
        let prof = json!({"SPNVMeDataType":[{"_items":[{"bsd_name":"disk0","device_serial":"ABC123","device_revision":"2914"}]}],
            "SPUSBDataType":[{"_items":[{"serial_num":"USB9","manufacturer":"SanDisk","Media":[{"bsd_name":"disk4"}]}]}]});
        let d = parse_drives(&list, &infos, &prof);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].id, "disk0");
        assert!(d[0].is_system_disk);
        assert_eq!(d[0].kind, DriveKind::Nvme);
        assert_eq!(d[0].serial.as_deref(), Some("ABC123"));
        assert_eq!(d[0].power_on_hours, Some(529));
        assert_eq!(d[1].id, "disk4");
        assert!(!d[1].is_system_disk);
        assert_eq!(d[1].kind, DriveKind::Usb);
        assert_eq!(d[1].serial.as_deref(), Some("USB9"));
        assert_eq!(d[1].mount_points, vec!["/Volumes/STICK"]);
        assert_eq!(d[1].smart_supported, Some(false));
        assert!(d[1].removable);
    }

    #[test]
    fn usb_identity_fallback_fills_only_gaps() {
        let mk = |id: &str, bus: &str, serial: Option<&str>| DriveInfo {
            id: id.into(), path: format!("/dev/r{id}"), model: None, serial: serial.map(String::from), firmware: None, vendor: None,
            capacity_bytes: 1, kind: DriveKind::Usb, bus: Some(bus.into()), rotation_rpm: None, removable: true, is_system_disk: false,
            mount_points: vec![], smart_supported: None, power_on_hours: None, manufacture_date: None,
        };
        let mut v = vec![mk("disk6", "usb", None), mk("disk7", "usb", Some("KEEP")), mk("disk0", "nvme", None)];
        let ident = |s: &str| usb::UsbIdent { serial: Some(s.into()), vendor: Some("V".into()), firmware: Some("1.00".into()) };
        let map: HashMap<String, usb::UsbIdent> = [("disk6", ident("A")), ("disk7", ident("B")), ("disk0", ident("C"))].into_iter().map(|(k, x)| (k.to_string(), x)).collect();
        fill_usb_identity(&mut v, &|| map.clone());
        assert_eq!(v[0].serial.as_deref(), Some("A"));
        assert_eq!(v[0].vendor.as_deref(), Some("V"));
        assert_eq!(v[1].serial.as_deref(), Some("KEEP"));
        assert_eq!(v[1].firmware.as_deref(), Some("1.00"));
        assert_eq!(v[2].serial, None, "non-USB untouched");
    }
}
