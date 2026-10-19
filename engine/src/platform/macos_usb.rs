//! macOS fallback identity for USB drives: vendor / serial / firmware from the IORegistry.
//!
//! `smartctl` cannot talk through most USB bridges on macOS and `system_profiler SPUSBDataType`
//! is empty on recent releases, but the IORegistry always knows the `IOUSBHostDevice` a disk hangs
//! under. `ioreg -r -c IOUSBHostDevice -l -w0` is unprivileged and takes about 0.2 s.

use crate::util::run_with_timeout;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UsbIdent {
    pub serial: Option<String>,
    pub vendor: Option<String>,
    pub firmware: Option<String>,
}

/// `"key" = "value"` -> (key, value) for string properties.
fn string_prop(line: &str) -> Option<(&str, &str)> {
    let t = line.trim_start_matches(['|', ' ']);
    let rest = t.strip_prefix('"')?;
    let (key, rest) = rest.split_once('"')?;
    let v = rest.trim_start().strip_prefix('=')?.trim();
    let v = v.strip_prefix('"')?.strip_suffix('"')?;
    Some((key, v))
}

fn int_prop(line: &str, name: &str) -> Option<u64> {
    let t = line.trim_start_matches(['|', ' ']);
    let rest = t.strip_prefix('"')?.strip_prefix(name)?.strip_prefix('"')?;
    rest.trim_start().strip_prefix('=')?.trim().parse().ok()
}

fn is_whole_disk(name: &str) -> bool {
    name.strip_prefix("disk").map(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
}

/// Vendor strings that carry no information.
fn useful_vendor(v: &str) -> bool {
    let l = v.trim().to_lowercase();
    !(l.is_empty() || l == "usb" || l == "generic" || l == "unknown" || l == "vendorco")
}

/// Serials that carry no information (empty or all zeros).
fn useful_serial(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && !t.chars().all(|c| c == '0')
}

/// bcdDevice 0x0512 -> "5.12".
fn bcd_version(v: u64) -> String {
    format!("{:x}.{:02x}", (v >> 8) & 0xff, v & 0xff)
}

/// Pure parser (unit-tested): whole-disk BSD name -> identity of the nearest USB device above it.
pub fn parse_ioreg(text: &str) -> HashMap<String, UsbIdent> {
    struct Node {
        col: usize,
        is_usb: bool,
        ident: UsbIdent,
    }
    let mut out = HashMap::new();
    let mut stack: Vec<Node> = Vec::new();
    for line in text.lines() {
        if let Some(col) = line.find("+-o ") {
            while stack.last().map(|n| n.col >= col).unwrap_or(false) {
                stack.pop();
            }
            stack.push(Node { col, is_usb: line.contains("<class IOUSBHostDevice,"), ident: UsbIdent::default() });
            continue;
        }
        let Some(cur) = stack.last_mut() else { continue };
        if cur.is_usb {
            if let Some((k, v)) = string_prop(line) {
                match k {
                    "kUSBSerialNumberString" | "USB Serial Number" if useful_serial(v) && cur.ident.serial.is_none() => cur.ident.serial = Some(v.trim().to_string()),
                    "USB Vendor Name" | "kUSBVendorString" if useful_vendor(v) && cur.ident.vendor.is_none() => cur.ident.vendor = Some(v.trim().to_string()),
                    _ => {}
                }
            } else if let Some(b) = int_prop(line, "bcdDevice") {
                cur.ident.firmware = Some(bcd_version(b));
            }
        }
        if let Some(("BSD Name", name)) = string_prop(line) {
            if is_whole_disk(name) {
                if let Some(usb) = stack.iter().rev().find(|n| n.is_usb) {
                    out.insert(name.to_string(), usb.ident.clone());
                }
            }
        }
    }
    out
}

/// Run ioreg (best effort, never fails the caller).
pub fn read_idents() -> HashMap<String, UsbIdent> {
    run_with_timeout(Path::new("/usr/sbin/ioreg"), &["-r", "-c", "IOUSBHostDevice", "-l", "-w0"], Duration::from_secs(5))
        .ok()
        .filter(|o| !o.timed_out)
        .map(|o| parse_ioreg(&o.stdout))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/ioreg_usb_macos26.txt");

    #[test]
    fn real_macos26_stick() {
        let m = parse_ioreg(FIXTURE);
        let d = m.get("disk6").expect("disk6 found");
        assert_eq!(d.serial.as_deref(), Some("1234567890123456789"));
        assert_eq!(d.firmware.as_deref(), Some("2.00"));
        assert_eq!(d.vendor, None); // bridge reports the generic vendor "USB"
        assert!(!m.contains_key("disk6s1"), "partitions are not whole disks");
        assert_eq!(m.len(), 1, "hub / keyboard carry no disk");
    }

    #[test]
    fn nearest_usb_ancestor_wins_and_vendor_kept() {
        let t = "\
+-o Hub@1  <class IOUSBHostDevice, id 0x1, registered>
  |   \"USB Vendor Name\" = \"Apple\"
  |   \"kUSBSerialNumberString\" = \"HUB1\"
  +-o Stick@2  <class IOUSBHostDevice, id 0x2, registered>
  | |   \"USB Vendor Name\" = \"SanDisk\"
  | |   \"kUSBSerialNumberString\" = \"0000000000\"
  | |   \"bcdDevice\" = 4128
  | +-o Media  <class IOMedia, id 0x3, registered>
  | |   \"BSD Name\" = \"disk9\"
";
        let d = &parse_ioreg(t)["disk9"];
        assert_eq!(d.vendor.as_deref(), Some("SanDisk"));
        assert_eq!(d.serial, None); // all-zero serial is noise, and the hub's must not leak down
        assert_eq!(d.firmware.as_deref(), Some("10.20"));
    }

    #[test]
    fn empty_and_garbage() {
        assert!(parse_ioreg("").is_empty());
        assert!(parse_ioreg("not ioreg output\n\"BSD Name\" = \"disk1\"").is_empty());
    }
}
