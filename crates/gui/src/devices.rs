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
        let is_usb = fs::canonicalize(&base)
            .map(|p| p.to_string_lossy().contains("/usb"))
            .unwrap_or(false);
        if !include_fixed && !removable && !is_usb {
            continue;
        }

        let size = read_u64(&base.join("size")).unwrap_or(0) * 512;
        if size == 0 {
            continue; // empty card reader, etc.
        }

        let model = device_name(&base).unwrap_or_else(|| name.clone());

        devices.push(DeviceInfo {
            path: format!("/dev/{name}"),
            model,
            size,
            removable: removable || is_usb,
        });
    }

    devices.sort_by(|a, b| a.path.cmp(&b.path));
    devices
}

/// Build the human-readable hardware name from sysfs `device/vendor` and
/// `device/model`. The vendor is prepended when it is meaningful — SATA disks
/// report a useless `ATA`, and it is dropped when the model already repeats it.
fn device_name(base: &Path) -> Option<String> {
    let model = read_trimmed(&base.join("device/model"))?;
    match read_trimmed(&base.join("device/vendor")) {
        Some(vendor)
            if !vendor.eq_ignore_ascii_case("ATA")
                && !model.to_lowercase().starts_with(&vendor.to_lowercase()) =>
        {
            Some(format!("{vendor} {model}"))
        }
        _ => Some(model),
    }
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
