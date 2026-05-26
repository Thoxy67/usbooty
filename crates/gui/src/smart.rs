//! Best-effort SMART health probe via the `smartctl` CLI.
//!
//! The kernel doesn't expose SMART through sysfs, so we shell out. We parse
//! the JSON output (`smartctl --json --info --health --attributes`) and pull
//! out the handful of fields that matter for "this stick is dying" warnings:
//!
//! * Overall health (`smart_status.passed == false`)
//! * Reallocated sectors (USB SSDs / NVMe behind enclosures)
//! * Temperature warning / critical
//! * Bad-blocks / current pending sectors
//!
//! The probe is fire-and-forget: a missing `smartctl`, a USB enclosure that
//! doesn't expose SMART (most cheap sticks), or a permission error all return
//! `None` and the UI shows nothing — same outcome as a healthy device.

use std::process::Command;

use serde_json::Value;

/// Run `smartctl` and return a one-line warning, or `None` when the device
/// looks healthy / SMART isn't available / smartmontools isn't installed.
///
/// Returns immediately (no progress reporting) — the typical probe takes
/// 50-200 ms on a USB SSD. Run from a worker thread.
pub fn probe(device_path: &str) -> Option<String> {
    let output = Command::new("smartctl")
        .args(["--json=c", "--info", "--health", "--attributes"])
        .arg(device_path)
        .output()
        .ok()?;
    // smartctl exits non-zero when SMART isn't supported by the underlying
    // device (very common on cheap USB sticks); the JSON body is still
    // emitted, so don't bail on the exit code.
    let json: Value = serde_json::from_slice(&output.stdout).ok()?;

    // If SMART is unsupported, the `device` block carries no `smart_support`
    // or `smart_status` — skip silently.
    let smart_supported = json
        .get("smart_support")
        .and_then(|v| v.get("available"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !smart_supported && json.get("smart_status").is_none() {
        return None;
    }

    let mut warnings = Vec::new();

    // 1. Overall pass/fail predicate.
    if let Some(passed) = json
        .get("smart_status")
        .and_then(|v| v.get("passed"))
        .and_then(Value::as_bool)
        && !passed
    {
        warnings.push("SMART overall health = FAILING".to_string());
    }

    // 2. Reallocated sectors (Attribute 5).
    if let Some(reallocated) = attribute_raw(&json, 5)
        && reallocated > 0
    {
        warnings.push(format!("{reallocated} reallocated sector(s)"));
    }

    // 3. Current pending sectors (Attribute 197) — sectors the disk is about
    //    to reallocate. Treat any non-zero count as a warning.
    if let Some(pending) = attribute_raw(&json, 197)
        && pending > 0
    {
        warnings.push(format!("{pending} pending bad sector(s)"));
    }

    // 4. Uncorrectable sectors (Attribute 198).
    if let Some(uncorrectable) = attribute_raw(&json, 198)
        && uncorrectable > 0
    {
        warnings.push(format!("{uncorrectable} uncorrectable sector(s)"));
    }

    // 5. Temperature warning (smartctl exposes a normalised `temperature`
    //    block on modern outputs). Flag anything over 60 °C — well below the
    //    typical 70-85 °C critical threshold but enough to alert the user.
    if let Some(temp) = json
        .get("temperature")
        .and_then(|v| v.get("current"))
        .and_then(Value::as_i64)
        && temp >= 60
    {
        warnings.push(format!("temperature {temp} °C"));
    }

    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join(" · "))
    }
}

/// Look up a SMART attribute by ID and return its raw value as a u64.
/// Returns None when the attribute isn't present in the report.
fn attribute_raw(json: &Value, id: u64) -> Option<u64> {
    let table = json.get("ata_smart_attributes")?.get("table")?.as_array()?;
    for attr in table {
        if attr.get("id").and_then(Value::as_u64) == Some(id) {
            return attr
                .get("raw")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_u64);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(reallocated: u64, pending: u64, uncorr: u64, temp: i64, passed: bool) -> Value {
        json!({
            "smart_support": { "available": true, "enabled": true },
            "smart_status": { "passed": passed },
            "temperature": { "current": temp },
            "ata_smart_attributes": {
                "table": [
                    { "id": 5,   "raw": { "value": reallocated } },
                    { "id": 197, "raw": { "value": pending } },
                    { "id": 198, "raw": { "value": uncorr } },
                ]
            }
        })
    }

    #[test]
    fn attribute_raw_returns_known_attribute() {
        let v = fixture(7, 0, 0, 35, true);
        assert_eq!(attribute_raw(&v, 5), Some(7));
        assert_eq!(attribute_raw(&v, 197), Some(0));
    }

    #[test]
    fn attribute_raw_returns_none_for_missing_id() {
        let v = fixture(0, 0, 0, 35, true);
        assert_eq!(attribute_raw(&v, 191), None);
    }

    #[test]
    fn attribute_raw_returns_none_when_table_missing() {
        let v = json!({ "smart_status": { "passed": true } });
        assert_eq!(attribute_raw(&v, 5), None);
    }
}
