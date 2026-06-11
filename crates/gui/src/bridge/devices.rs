//! `AppController` methods for device enumeration, selection, the per-device
//! readouts the confirm dialog binds to, inspection, and ejection.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::DeviceInfo;

use super::helpers::{collect_inspect_text, compute_max_persistence_mib};
use super::qobject;

impl qobject::AppController {
    /// Re-scan `/sys/block` for candidate target devices.
    pub fn refresh_devices(mut self: core::pin::Pin<&mut Self>) {
        let include_fixed = *self.show_fixed_disks();
        let devices = crate::devices::enumerate(include_fixed);

        // Remember the selected device by its kernel path so an auto-refresh
        // that returns the same drive keeps it selected, instead of snapping
        // the combo back to the first entry every couple of seconds.
        let prev_path = {
            let idx = *self.selected_device();
            if idx >= 0 {
                self.rust()
                    .device_list
                    .get(idx as usize)
                    .map(|d| d.path.clone())
            } else {
                None
            }
        };

        let display = QString::from(
            &devices
                .iter()
                .map(DeviceInfo::display)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        // Only rewrite the model when it actually changed; a no-op refresh must
        // not rebuild the combo's model (which is what disturbed the selection).
        if *self.devices() != display {
            self.as_mut().set_devices(display);
        }

        // Re-find the previously-selected device; fall back to the first entry
        // only when it is gone (or nothing was selected yet).
        let selected = prev_path
            .clone()
            .and_then(|path| devices.iter().position(|d| d.path == path))
            .map(|i| i as i32)
            .unwrap_or(if devices.is_empty() { -1 } else { 0 });
        if *self.selected_device() != selected {
            self.as_mut().set_selected_device(selected);
        }
        // The selection landed on a *different* device (the old one is gone,
        // or nothing was selected before): any SMART warning on screen
        // belongs to the previous device, not this one.
        let now_path = (selected >= 0)
            .then(|| devices.get(selected as usize).map(|d| d.path.clone()))
            .flatten();
        if now_path != prev_path {
            self.as_mut().set_smart_warning(QString::default());
        }

        self.as_mut().rust_mut().device_list = devices;
        self.as_mut().refresh_fit_warning();
        self.as_mut().refresh_persistence_max();
    }

    /// Select a target device by index, then refresh the capacity warning
    /// and kick off a background SMART probe of the chosen device.
    pub fn select_device(mut self: core::pin::Pin<&mut Self>, index: i32) {
        self.as_mut().set_selected_device(index);
        self.as_mut().set_smart_warning(QString::default());
        // Breadcrumb the target into the activity log so a saved log shows
        // exactly which device the user pointed the job at.
        if let Some(dev) = self.selected_info().cloned() {
            self.as_mut().log_info(&format!(
                "Target device selected: {} ({}, {})",
                dev.path,
                dev.model.trim(),
                usbooty_core::device::format_size(dev.size),
            ));
        }
        self.as_mut().refresh_fit_warning();
        self.as_mut().refresh_persistence_max();
        self.as_mut().probe_smart();
    }

    /// Recompute the slider's max from the *currently selected* device's
    /// free space (size − ISO − 64 MiB filesystem margin) and clamp the
    /// current value if the new ceiling fell below it. Called whenever the
    /// device selection, the device list, or the loaded ISO changes, that
    /// way the slider's `to:` is always exactly what will fit on the chosen
    /// drive, never a stale 32 GiB hard-cap.
    pub(crate) fn refresh_persistence_max(mut self: core::pin::Pin<&mut Self>) {
        let max_mib =
            compute_max_persistence_mib(self.selected_info(), self.rust().iso_report.as_ref());
        self.as_mut().set_persistence_max_mib(max_mib);
        // If the slider sat above the new ceiling (smaller device just
        // picked, or a larger ISO loaded), pull it down. Leave 0 alone so
        // an explicitly-off slider doesn't pop back to a non-zero value.
        let current = *self.persistence_size();
        if current > max_mib {
            self.as_mut().set_persistence_size(max_mib.max(0));
        }
    }

    /// Spawn a background thread that runs `smartctl --json` against the
    /// currently-selected device and publishes any warning to
    /// `smart_warning`. Silent when smartmontools isn't installed.
    fn probe_smart(self: core::pin::Pin<&mut Self>) {
        let Some(device) = self.selected_info().cloned() else {
            return;
        };
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let warning = crate::smart::probe(&device.path).unwrap_or_default();
            if warning.is_empty() {
                return;
            }
            let _ = qt.queue(
                move |mut ctrl: core::pin::Pin<&mut qobject::AppController>| {
                    // A slow probe can land after the user moved to another
                    // device; only publish if the probed path is still the
                    // selected one, so device B never wears device A's warning.
                    if ctrl.selected_info().map(|d| d.path.as_str()) != Some(device.path.as_str()) {
                        return;
                    }
                    ctrl.as_mut().set_smart_warning(QString::from(&warning));
                },
            );
        });
    }

    /// Recompute [`fit_warning`](Self::fit_warning) from the current ISO and
    /// selected device; set to a message when the image cannot possibly fit.
    pub(crate) fn refresh_fit_warning(mut self: core::pin::Pin<&mut Self>) {
        let iso_bytes = self.rust().iso_report.as_ref().map_or(0, |r| r.total_size);
        let device = self
            .selected_info()
            .map(|d| (d.model_name().to_string(), d.size));

        let warning = match device {
            Some((model, size)) if iso_bytes > 0 && size > 0 && iso_bytes > size => format!(
                "This image ({}) is larger than {model} ({}) and will not fit.",
                usbooty_core::device::format_size(iso_bytes),
                usbooty_core::device::format_size(size),
            ),
            _ => String::new(),
        };
        self.as_mut().set_fit_warning(QString::from(&warning));
    }

    /// The [`DeviceInfo`] for the current `selected_device` index, if valid.
    pub(crate) fn selected_info(&self) -> Option<&DeviceInfo> {
        let idx = *self.selected_device();
        if idx < 0 {
            None
        } else {
            self.rust().device_list.get(idx as usize)
        }
    }

    // ---- Selected-device accessors used by the confirm dialog ---------------

    pub fn selected_model(&self) -> QString {
        QString::from(self.selected_info().map(|d| d.model_name()).unwrap_or(""))
    }

    pub fn selected_size_text(&self) -> QString {
        QString::from(
            self.selected_info()
                .map(|d| usbooty_core::device::format_size(d.size))
                .unwrap_or_default(),
        )
    }

    pub fn selected_path(&self) -> QString {
        QString::from(self.selected_info().map(|d| d.path.as_str()).unwrap_or(""))
    }

    pub fn selected_is_internal(&self) -> bool {
        self.selected_info().is_some_and(|d| !d.removable)
    }

    pub fn selected_bus(&self) -> QString {
        QString::from(
            self.selected_info()
                .and_then(|d| d.bus.as_deref())
                .unwrap_or(""),
        )
    }

    pub fn selected_serial(&self) -> QString {
        QString::from(
            self.selected_info()
                .and_then(|d| d.serial.as_deref())
                .unwrap_or(""),
        )
    }

    /// Compute the largest persistence size that still leaves room for the
    /// ISO + a 64 MiB partition-table / filesystem-overhead margin. Returns
    /// 0 when the slider should stay disabled (no device, no ISO, no room).
    ///
    /// Kept as an invokable for the "Max" button, which wants the freshest
    /// value at the moment of the click, and as a thin wrapper around the
    /// same pure function `refresh_persistence_max` uses, so the property
    /// and the invokable can never drift apart.
    pub fn max_persistence_mib(&self) -> i32 {
        compute_max_persistence_mib(self.selected_info(), self.rust().iso_report.as_ref())
    }

    /// Trim the current label down to whatever fits on the chosen filesystem,
    /// matching what the helper will end up writing. Pure preview, no state
    /// change, surfaced as a tooltip on the volume-label field.
    pub fn sanitized_label(&self) -> QString {
        let label = self.label().to_string();
        let cleaned = match *self.filesystem() {
            // FAT32: 11 chars, upper-cased, no extended chars.
            0 => label
                .chars()
                .filter(|c| c.is_ascii() && !c.is_control())
                .take(11)
                .collect::<String>()
                .to_ascii_uppercase(),
            // NTFS: up to 32 chars (UTF-16 code units, kept simple here).
            1 => label.chars().take(32).collect::<String>(),
            // exFAT: 11 chars.
            2 => label.chars().take(11).collect::<String>(),
            // ext4: 16 bytes.
            3 => {
                let mut out = String::new();
                for c in label.chars() {
                    if out.len() + c.len_utf8() > 16 {
                        break;
                    }
                    out.push(c);
                }
                out
            }
            _ => label,
        };
        QString::from(&cleaned)
    }

    /// Kick off an off-thread inspect: lsblk + udevadm + smartctl. The
    /// dialog binds to [`inspect_text`](Self::inspect_text); we paint a
    /// "Loading…" placeholder immediately so the dialog can open right
    /// away instead of freezing for 50-500 ms while the children run.
    pub fn request_inspect(mut self: core::pin::Pin<&mut Self>) {
        let Some(device) = self.selected_info().cloned() else {
            self.as_mut().set_inspect_text(QString::default());
            return;
        };
        let path = device.path.clone();
        self.as_mut()
            .set_inspect_text(QString::from("Loading device details, please wait…"));
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let text = collect_inspect_text(&path);
            let _ = qt.queue(move |mut ctrl: core::pin::Pin<&mut Self>| {
                // Two rapid inspects race; only the one matching the current
                // selection may paint the dialog (same guard as probe_smart).
                if ctrl.selected_info().map(|d| d.path.as_str()) != Some(path.as_str()) {
                    return;
                }
                ctrl.as_mut().set_inspect_text(QString::from(&text));
            });
        });
    }

    /// Try to power off the currently-selected USB device. Best-effort: prefers
    /// `udisksctl power-off` (the desktop standard, handles unmount + safe
    /// removal in one call), falling back to `eject -F` when udisksctl is
    /// missing *or* fails. Either tool runs as the user; no helper hop needed.
    /// Runs on a worker thread: `power-off` syncs dirty pages and routinely
    /// takes over a second, which would freeze the UI. The device list is
    /// refreshed on success so the now-detached device disappears from the
    /// combo.
    pub fn eject_device(mut self: core::pin::Pin<&mut Self>) {
        let Some(device) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("No device selected"));
            return;
        };
        let path = device.path.clone();
        self.as_mut()
            .set_status(QString::from(&format!("Ejecting {path}…")));
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let primary = std::process::Command::new("udisksctl")
                .args(["power-off", "-b", &path])
                .output();
            let outcome = if matches!(&primary, Ok(o) if o.status.success()) {
                Ok(())
            } else {
                match std::process::Command::new("eject")
                    .args(["-F", &path])
                    .output()
                {
                    Ok(o) if o.status.success() => Ok(()),
                    _ => {
                        // Both failed; udisksctl's stderr is the richer message.
                        Err(match &primary {
                            Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
                            Err(e) => e.to_string(),
                        })
                    }
                }
            };
            let _ = qt.queue(move |mut ctrl: core::pin::Pin<&mut Self>| match outcome {
                Ok(()) => {
                    ctrl.as_mut().log_info(&format!("Ejected {path}"));
                    ctrl.as_mut()
                        .set_status(QString::from(&format!("Ejected {path}")));
                    ctrl.refresh_devices();
                }
                Err(err) => {
                    ctrl.as_mut()
                        .log_warn(&format!("Eject failed for {path}: {err}"));
                    ctrl.as_mut()
                        .set_status(QString::from(&format!("Eject failed: {err}")));
                }
            });
        });
    }
}
