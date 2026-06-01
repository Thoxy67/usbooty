//! `AppController` invokables for app-level UI: logging, settings toggles,
//! boot verification, dependency reporting, and startup-arg replay.

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use super::qobject;

impl qobject::AppController {
    /// Record one activity-log line: keep the plain text for "Save log", keep
    /// the HTML for repopulating the lazily-loaded view, flip the non-empty
    /// flag on the first line, and emit the new line for the live view to
    /// append. Each call is O(line length); no whole-buffer rebuild.
    pub fn push_log_line(mut self: core::pin::Pin<&mut Self>, plain: &str, html: &str) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.full_log.push_str(plain);
            rust.full_log.push('\n');
            rust.log_html.push_str(html);
        }
        if !*self.as_ref().log_non_empty() {
            self.as_mut().set_log_non_empty(true);
        }
        self.as_mut().append_log_html(QString::from(html));
    }

    /// Empty the activity log. The view clears itself when `log_non_empty`
    /// flips to false.
    pub fn clear_log(mut self: core::pin::Pin<&mut Self>) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.full_log.clear();
            rust.log_html.clear();
        }
        self.as_mut().set_log_non_empty(false);
    }

    /// The full activity log as HTML, for the QML view to repopulate on load.
    pub fn log_html_snapshot(&self) -> QString {
        QString::from(&self.rust().log_html)
    }

    /// Apply CLI startup args after the QML engine has loaded.
    ///
    /// `--device` is matched against the freshly-enumerated device list (so a
    /// USB stick that wasn't plugged in until just before launch still gets
    /// found). `--iso` runs through the regular `set_iso` path, which spawns
    /// off-thread decompression for compressed images.
    pub fn apply_startup_args(mut self: core::pin::Pin<&mut Self>) {
        let Some(args) = crate::cli::take() else {
            return;
        };
        if let Some(device) = &args.device {
            let want = device.to_string_lossy();
            let index = self
                .rust()
                .device_list
                .iter()
                .position(|d| d.path == want.as_ref())
                .map(|i| i as i32);
            if let Some(index) = index {
                self.as_mut().select_device(index);
            } else {
                self.as_mut().set_status(QString::from(&format!(
                    "Device {want} from --device is not present"
                )));
            }
        }
        if let Some(iso) = &args.iso {
            let path = QString::from(iso.to_string_lossy().as_ref());
            self.set_iso(&path);
        }
    }

    /// Boot the selected device in QEMU (BIOS/MBR or UEFI) to verify it boots.
    /// Spawns QEMU and returns; outcome is surfaced on the status bar.
    pub fn verify_boot(
        mut self: core::pin::Pin<&mut Self>,
        mem_mb: i32,
        cpus: i32,
        firmware: i32,
        q35: bool,
        audio: bool,
        kvm: bool,
        network: bool,
        snapshot: bool,
    ) {
        let Some(path) = self.selected_info().map(|d| d.path.clone()) else {
            self.as_mut()
                .set_status(QString::from("Select a device to boot-test first"));
            return;
        };
        let cfg = crate::qemu::BootConfig {
            mem_mb: mem_mb.max(0) as u32,
            cpus: cpus.max(1) as u32,
            firmware: firmware.clamp(0, 2) as u32,
            q35,
            audio,
            kvm,
            network,
            snapshot,
        };
        // Collect QEMU's log lines (full command, env, swtpm, any startup
        // error) during the call, then flush them to the activity log.
        let mut lines: Vec<String> = Vec::new();
        let result = crate::qemu::launch(&path, &cfg, &mut |line| lines.push(line.to_string()));
        for line in &lines {
            let html = crate::runner::log_html(usbooty_core::LogLevel::Info, line);
            self.as_mut().push_log_line(line, &html);
        }
        match result {
            Ok(()) => self.as_mut().set_status(QString::from(&format!(
                "Launched QEMU boot test for {path}{}",
                if snapshot {
                    " (snapshot mode: the device is not modified)"
                } else {
                    " (writes persist: the device IS modified)"
                }
            ))),
            Err(e) => {
                let msg = format!("Boot test failed: {e:#}");
                let html = crate::runner::log_html(usbooty_core::LogLevel::Error, &msg);
                self.as_mut().push_log_line(&msg, &html);
                self.as_mut().set_status(QString::from(&format!(
                    "Could not start the boot test: {e:#}"
                )));
            }
        }
    }

    /// Live dependency status for the Dependencies dialog (see the qinvokable
    /// declaration for the line format). Probed fresh on every call so the
    /// dialog reflects tools installed since launch.
    pub fn dependency_report(&self) -> QString {
        let lines: Vec<String> = crate::deps::full_report()
            .iter()
            .map(|d| {
                format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                    u8::from(d.present),
                    d.kind_key,
                    d.name,
                    d.package,
                    d.purpose,
                )
            })
            .collect();
        QString::from(&lines.join("\n"))
    }

    /// Write the activity log buffer to a user-chosen file. Strips a
    /// leading `file://` (QML FileDialog returns URLs even for local
    /// paths) and reports the outcome via the status bar so the user
    /// gets feedback without a modal popup.
    pub fn save_log_to(mut self: core::pin::Pin<&mut Self>, path: &QString) {
        let raw = path.to_string();
        let path = raw.strip_prefix("file://").unwrap_or(&raw).to_string();
        if path.is_empty() {
            return;
        }
        let body = self.rust().full_log.clone();
        match std::fs::write(&path, body.as_bytes()) {
            Ok(()) => self
                .as_mut()
                .set_status(QString::from(&format!("Activity log saved to {path}"))),
            Err(e) => self.as_mut().set_status(QString::from(&format!(
                "Could not save activity log to {path}: {e}"
            ))),
        }
    }

    /// Persist the *force English* state on the controller and route the
    /// underlying QTranslator swap to the translation module. Qt emits a
    /// LanguageChange event from removeTranslator/installTranslator, so the
    /// UI re-evaluates every `qsTr` binding and the swap is visible
    /// immediately.
    pub fn apply_force_english(mut self: core::pin::Pin<&mut Self>, force: bool) {
        self.as_mut().set_force_english(force);
        crate::translations::set_force_english(force);
        // Persist the choice. Round-trip the existing struct with the
        // new value so any other persisted fields keep their state.
        let s = crate::settings::Settings {
            force_english: force,
            show_logs_always: *self.show_logs_always(),
            log_all_files: *self.log_all_files(),
        };
        if let Err(e) = s.save() {
            let msg = format!("Could not persist 'Force English' preference: {e:#}");
            let html = crate::runner::log_html(usbooty_core::LogLevel::Warn, &msg);
            self.as_mut().push_log_line(&msg, &html);
        }
    }

    /// Save the "always show activity log" toggle and update the
    /// matching Qt property. The QML layout binds to `show_logs_always`
    /// so the panel appears / disappears immediately.
    pub fn apply_show_logs_always(mut self: core::pin::Pin<&mut Self>, on: bool) {
        self.as_mut().set_show_logs_always(on);
        let s = crate::settings::Settings {
            force_english: *self.force_english(),
            show_logs_always: on,
            log_all_files: *self.log_all_files(),
        };
        if let Err(e) = s.save() {
            let msg = format!("Could not persist 'Always show logs' preference: {e:#}");
            let html = crate::runner::log_html(usbooty_core::LogLevel::Warn, &msg);
            self.as_mut().push_log_line(&msg, &html);
        }
    }

    /// Save the "log every copied file" toggle and update the matching Qt
    /// property. Each subsequent job reads it into its options, so the helper
    /// names every file in the activity log instead of only the large ones.
    pub fn apply_log_all_files(mut self: core::pin::Pin<&mut Self>, on: bool) {
        self.as_mut().set_log_all_files(on);
        let s = crate::settings::Settings {
            force_english: *self.force_english(),
            show_logs_always: *self.show_logs_always(),
            log_all_files: on,
        };
        if let Err(e) = s.save() {
            let msg = format!("Could not persist 'Log every file' preference: {e:#}");
            let html = crate::runner::log_html(usbooty_core::LogLevel::Warn, &msg);
            self.as_mut().push_log_line(&msg, &html);
        }
    }

    /// One-click "copy from system" for the Windows-setup locale + timezone
    /// fields. Reads `$LANG` / `$LC_ALL` for the BCP-47 locale, resolves
    /// `/etc/timezone` (or the `/etc/localtime` symlink) for the IANA zone,
    /// maps the IANA zone to its Microsoft `TimeZone` ID via the catalog
    /// we already ship, and writes the results back to the QML properties.
    /// Unknown IANA zones fall back to `UTC`; missing `$LANG` falls back to
    /// `en-US` so the autounattend always has something usable.
    pub fn replicate_regional_from_host(mut self: core::pin::Pin<&mut Self>) {
        let locale = crate::timezones::host_locale();
        let iana = crate::timezones::host_iana();
        let ms_tz = crate::timezones::from_iana(&iana).unwrap_or("UTC");
        self.as_mut().set_locale(QString::from(&locale));
        self.as_mut().set_timezone(QString::from(ms_tz));
        self.as_mut().set_status(QString::from(&format!(
            "Copied host regional settings: {locale} / {ms_tz}"
        )));
    }
}
