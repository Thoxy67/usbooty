//! Enumeration of candidate target block devices by reading `/sys/block`.
//!
//! This is dependency-free and reliable: the kernel exposes everything we need
//! (removable flag, size, model) as plain files, and the USB transport shows
//! up in the device's sysfs path.

use std::fs;
use std::path::Path;

use usbooty_core::DeviceInfo;

/// Whole-disk device-name prefixes that are never valid USB targets.
const SKIP_PREFIXES: [&str; 6] = ["loop", "ram", "zram", "sr", "dm-", "md"];

/// Enumerate target devices. When `include_fixed` is false, only removable or
/// USB-attached disks are returned.
pub fn enumerate(include_fixed: bool) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return devices;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if SKIP_PREFIXES.iter().any(|p| name.starts_with(p)) {
            continue;
        }

        let base = Path::new("/sys/block").join(&name);
        let removable = read_flag(&base.join("removable"));
        let canon = fs::canonicalize(&base).ok();
        let is_usb = canon
            .as_ref()
            .map(|p| p.to_string_lossy().contains("/usb"))
            .unwrap_or(false);
        if !include_fixed && !removable && !is_usb {
            continue;
        }

        let size = read_u64(&base.join("size")).unwrap_or(0) * 512;
        if size == 0 {
            continue; // empty card reader, etc.
        }

        // Combine the USB device descriptor (when present) with the SCSI/ATA
        // `device/model` so an enclosure that reports a blank model still
        // gets a useful name from the USB `product` field.
        let usb = canon.as_ref().and_then(|p| usb_descriptor(p));
        let (model, vendor) = pick_name(&base, usb.as_ref());
        let bus = classify_bus(&name, usb.as_ref());
        let serial = usb.as_ref().and_then(|u| u.serial.clone());

        devices.push(DeviceInfo {
            path: format!("/dev/{name}"),
            model,
            size,
            removable: removable || is_usb,
            bus,
            serial,
            vendor,
        });
    }

    devices.sort_by(|a, b| a.path.cmp(&b.path));
    devices
}

/// USB device descriptor fields read from the parent USB node.
#[derive(Debug, Default)]
struct UsbDescriptor {
    manufacturer: Option<String>,
    product: Option<String>,
    serial: Option<String>,
    /// Negotiated bus speed in Mbps as reported by the kernel
    /// (12 = USB 1.1 FS, 480 = USB 2.0 HS, 5000 = USB 3.0, 10000 = USB 3.1, …).
    speed_mbps: Option<u32>,
}

/// Walk up from `start` (typically the canonical path of `/sys/block/sdX`)
/// looking for a directory with `idVendor` + `idProduct` — that's the USB
/// device node and where the human-readable descriptor fields live.
fn usb_descriptor(start: &Path) -> Option<UsbDescriptor> {
    let mut cur = start.parent()?;
    for _ in 0..16 {
        if cur.join("idVendor").is_file() {
            return Some(UsbDescriptor {
                manufacturer: read_trimmed(&cur.join("manufacturer")),
                product: read_trimmed(&cur.join("product")),
                serial: read_trimmed(&cur.join("serial")),
                speed_mbps: read_trimmed(&cur.join("speed")).and_then(|s| s.parse().ok()),
            });
        }
        cur = cur.parent()?;
    }
    None
}

/// Choose the best name + vendor pair for the device. The USB descriptor's
/// `product` is preferred (it's what shows up in `lsusb`), with the SCSI
/// `device/model` as a fallback. Returns `(combined_display_name, vendor)`.
fn pick_name(base: &Path, usb: Option<&UsbDescriptor>) -> (String, Option<String>) {
    let scsi_model = read_trimmed(&base.join("device/model"));
    let scsi_vendor =
        read_trimmed(&base.join("device/vendor")).filter(|v| !v.eq_ignore_ascii_case("ATA"));

    // Prefer USB descriptor strings — they're the names a user recognises.
    let usb_product = usb.and_then(|u| u.product.clone());
    let usb_vendor = usb.and_then(|u| u.manufacturer.clone());

    let model = usb_product
        .clone()
        .or_else(|| scsi_model.clone())
        .filter(|s| !s.is_empty());
    let vendor = usb_vendor.clone().or(scsi_vendor.clone());

    let display = match (vendor.as_deref(), model.as_deref()) {
        (Some(v), Some(m))
            if !m.to_lowercase().starts_with(&v.to_lowercase()) && !v.eq_ignore_ascii_case(m) =>
        {
            format!("{v} {m}")
        }
        (_, Some(m)) => m.to_string(),
        (Some(v), None) => v.to_string(),
        (None, None) => String::new(),
    };
    (display, vendor)
}

/// Map the device name and USB speed to a short bus label.
fn classify_bus(name: &str, usb: Option<&UsbDescriptor>) -> Option<String> {
    if let Some(u) = usb {
        let speed = match u.speed_mbps {
            Some(12) => "USB 1.1",
            Some(480) => "USB 2.0",
            Some(5000) => "USB 3.0",
            Some(10000) => "USB 3.1",
            Some(20000) => "USB 3.2",
            _ => "USB",
        };
        return Some(speed.to_string());
    }
    if name.starts_with("nvme") {
        return Some("NVMe".to_string());
    }
    if name.starts_with("mmcblk") {
        return Some("SD/eMMC".to_string());
    }
    if name.starts_with("sd") {
        return Some("SATA".to_string());
    }
    None
}

/// Read a sysfs file, trimmed, returning `None` when missing or empty.
fn read_trimmed(path: &Path) -> Option<String> {
    read_text(path)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read a sysfs boolean flag file (`"1"` => true).
fn read_flag(path: &Path) -> bool {
    read_text(path).map(|s| s.trim() == "1").unwrap_or(false)
}

/// Read a sysfs file as a `u64`.
fn read_u64(path: &Path) -> Option<u64> {
    read_text(path)?.trim().parse().ok()
}

/// Read a sysfs file as a UTF-8 string.
fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}
