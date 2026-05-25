//! Description of a candidate target block device, as enumerated by the GUI.

use serde::{Deserialize, Serialize};

/// A block device the user could write to (typically a removable USB drive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Kernel device node, e.g. `/dev/sdb`.
    pub path: String,
    /// Human-readable vendor/model string, e.g. `SanDisk Ultra`. May be a
    /// placeholder when the underlying USB enclosure under-reports.
    pub model: String,
    /// Total capacity in bytes.
    pub size: u64,
    /// Whether the kernel reports the device as removable.
    pub removable: bool,
    /// Bus + speed summary, e.g. `USB 3.0`, `USB 2.0`, `NVMe`, `SATA`. None
    /// when the enumerator couldn't classify it.
    #[serde(default)]
    pub bus: Option<String>,
    /// USB / SCSI serial number — useful for telling two identical sticks apart.
    #[serde(default)]
    pub serial: Option<String>,
    /// USB vendor name as reported by the device descriptor (separate from
    /// `model` so the UI can show both, or skip duplicates).
    #[serde(default)]
    pub vendor: Option<String>,
}

impl DeviceInfo {
    /// A label for the device picker, e.g.
    /// `SanDisk Ultra — 30.8 GB · USB 3.0 · /dev/sdb`.
    ///
    /// The em-dash separates a primary part (the model) from a detail part
    /// (capacity, bus, node); the QML delegate splits on it to render the
    /// two on separate rows.
    pub fn display(&self) -> String {
        format!("{} — {}", self.model_name(), self.detail())
    }

    /// The model name, falling back to a placeholder when the kernel reports none.
    pub fn model_name(&self) -> &str {
        let model = self.model.trim();
        if model.is_empty() {
            "Unknown device"
        } else {
            model
        }
    }

    /// The secondary detail line: capacity, bus, serial (when present), and
    /// device node. Falls back to "Removable" / "Internal disk" when no
    /// specific bus is known. Serial is included because it's the easiest
    /// way to tell two identical USB sticks apart in the dropdown.
    pub fn detail(&self) -> String {
        let bus = match (&self.bus, self.removable) {
            (Some(b), _) => b.as_str(),
            (None, true) => "Removable",
            (None, false) => "Internal disk",
        };
        let mut parts = vec![format_size(self.size), bus.to_string()];
        if let Some(s) = self.serial.as_ref().filter(|s| !s.is_empty()) {
            // Short serials show in full; long enclosure serials get truncated
            // so the dropdown row doesn't bloat past the combo's width.
            let shown = if s.len() > 14 { &s[..14] } else { s.as_str() };
            parts.push(format!("S/N {shown}"));
        }
        parts.push(self.path.clone());
        parts.join(" · ")
    }
}

/// Format a byte count as a short decimal (SI) size string.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(30_752_000_000), "30.8 GB");
    }
}
