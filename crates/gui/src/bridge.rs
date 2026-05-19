//! The `AppController` QObject — the single object QML binds to.
//!
//! Properties hold all UI state; invokables are the actions QML triggers. The
//! heavy lifting (device enumeration, running the privileged helper) lives in
//! sibling modules; this file is the Qt-facing surface.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::{
    DeviceInfo, FileSystem, IsoReport, Job, JobOptions, OsKind, PartitionTable, Persistence,
    WimStrategy, WindowsSetup,
};

/// The bridge module exposed to C++/QML.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        // Source ISO.
        #[qproperty(QString, iso_path)]
        #[qproperty(QString, iso_summary)]
        // Editable volume label, pre-filled from the ISO's own label.
        #[qproperty(QString, label)]
        // SHA-256 of the source ISO ("Computing…" while it is calculated).
        #[qproperty(QString, iso_sha256)]
        // The application version, for the About dialog.
        #[qproperty(QString, app_version)]
        // Target devices: newline-separated display strings for the combo box.
        #[qproperty(QString, devices)]
        #[qproperty(i32, selected_device)]
        #[qproperty(bool, show_fixed_disks)]
        // Options. method: 0 = DD, 1 = partition & copy, 2 = format only.
        // table: 0 = GPT, 1 = MBR. filesystem (format mode only):
        // 0 = FAT32, 1 = NTFS, 2 = exFAT, 3 = ext4.
        #[qproperty(i32, method)]
        #[qproperty(i32, table)]
        #[qproperty(i32, filesystem)]
        // Ventoy options (write method 3).
        #[qproperty(bool, ventoy_update)]
        #[qproperty(bool, ventoy_secure_boot)]
        // Zero the whole device before writing, rather than a quick format.
        #[qproperty(bool, full_format)]
        // Read the written data back and verify it after the job.
        #[qproperty(bool, verify)]
        // Linux live-USB persistence: whether the ISO supports it, and the
        // chosen overlay size in MiB (0 = no persistence partition).
        #[qproperty(bool, persistence_supported)]
        #[qproperty(i32, persistence_size)]
        // Windows 11 installer customization (applied via autounattend.xml).
        // `windows_iso` / `linux_iso` reflect the detected OS of the source ISO.
        #[qproperty(bool, windows_iso)]
        #[qproperty(bool, linux_iso)]
        #[qproperty(bool, bypass_tpm)]
        #[qproperty(bool, bypass_secureboot)]
        #[qproperty(bool, bypass_ram)]
        #[qproperty(bool, skip_msaccount)]
        #[qproperty(bool, disable_telemetry)]
        #[qproperty(QString, local_account)]
        // Job state.
        #[qproperty(bool, busy)]
        #[qproperty(f64, progress)]
        #[qproperty(QString, phase)]
        #[qproperty(QString, log_text)]
        #[qproperty(QString, status)]
        // Live transfer stats (empty unless a transfer is in progress).
        #[qproperty(QString, speed)]
        #[qproperty(QString, eta)]
        // Non-empty when the selected ISO is too large for the chosen device.
        #[qproperty(QString, fit_warning)]
        // Advisory warning about missing external tools (empty if all present).
        #[qproperty(QString, dep_warning)]
        // Windows-download dialog: newline-separated language / option lists.
        #[qproperty(QString, win_languages)]
        #[qproperty(QString, win_options)]
        type AppController = super::AppControllerRust;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Re-scan the system for target block devices.
        #[qinvokable]
        fn refresh_devices(self: Pin<&mut AppController>);
        /// Select a target device by index and re-evaluate the capacity check.
        #[qinvokable]
        fn select_device(self: Pin<&mut AppController>, index: i32);
        /// Set the source ISO from a path or `file://` URL.
        #[qinvokable]
        fn set_iso(self: Pin<&mut AppController>, path: &QString);
        /// Validate inputs and launch the privileged helper.
        #[qinvokable]
        fn start(self: Pin<&mut AppController>);
        /// Request cancellation of the running job.
        #[qinvokable]
        fn cancel(self: Pin<&mut AppController>);
        /// Whether a confirmation dialog should be shown before starting.
        #[qinvokable]
        fn can_start(self: &AppController) -> bool;
        /// A human-readable description of the device about to be erased.
        #[qinvokable]
        fn confirm_text(self: &AppController) -> QString;
        /// Fetch the language list for a Windows release (by `RELEASES` index).
        #[qinvokable]
        fn win_fetch_languages(self: Pin<&mut AppController>, version_index: i32);
        /// Fetch the download options for a language (by index).
        #[qinvokable]
        fn win_fetch_options(self: Pin<&mut AppController>, language_index: i32);
        /// Download a Windows ISO option (by index) and select it as the source.
        #[qinvokable]
        fn win_download(self: Pin<&mut AppController>, option_index: i32);
        /// Open Microsoft's official download page in the system browser.
        #[qinvokable]
        fn open_microsoft_page(self: &AppController, version_index: i32);
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Emitted when a job finishes (success or failure).
        #[qsignal]
        fn job_finished(self: Pin<&mut AppController>, success: bool, message: QString);
    }

    impl cxx_qt::Threading for AppController {}
}

/// Handle to a running job, kept so [`AppController::cancel`] can reach it.
pub struct JobHandle {
    /// The helper's stdin — writing `cancel` here aborts it.
    pub stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
}

/// Backing storage for [`qobject::AppController`].
pub struct AppControllerRust {
    iso_path: QString,
    iso_summary: QString,
    label: QString,
    iso_sha256: QString,
    app_version: QString,
    devices: QString,
    selected_device: i32,
    show_fixed_disks: bool,
    method: i32,
    table: i32,
    filesystem: i32,
    ventoy_update: bool,
    ventoy_secure_boot: bool,
    full_format: bool,
    verify: bool,
    persistence_supported: bool,
    persistence_size: i32,
    windows_iso: bool,
    linux_iso: bool,
    bypass_tpm: bool,
    bypass_secureboot: bool,
    bypass_ram: bool,
    skip_msaccount: bool,
    disable_telemetry: bool,
    local_account: QString,
    busy: bool,
    progress: f64,
    phase: QString,
    log_text: QString,
    status: QString,
    speed: QString,
    eta: QString,
    fit_warning: QString,
    dep_warning: QString,
    win_languages: QString,
    win_options: QString,
    /// Enumerated devices, parallel to the `devices` display strings.
    device_list: Vec<DeviceInfo>,
    /// Analysis of the currently selected ISO.
    iso_report: Option<IsoReport>,
    /// Languages fetched for the Windows-download dialog.
    pub win_catalog: Option<crate::windisco::Catalog>,
    /// Download options fetched for the selected language.
    pub win_option_list: Vec<crate::windisco::DownloadOption>,
    /// Present while a job runs; cleared by the runner when it finishes.
    pub job: Option<JobHandle>,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        Self {
            iso_path: QString::default(),
            iso_summary: QString::from("No image selected"),
            label: QString::default(),
            iso_sha256: QString::default(),
            app_version: QString::from(env!("CARGO_PKG_VERSION")),
            devices: QString::default(),
            selected_device: -1,
            show_fixed_disks: false,
            method: 0,
            table: 0,
            filesystem: 0,
            ventoy_update: false,
            ventoy_secure_boot: true,
            full_format: false,
            verify: false,
            persistence_supported: false,
            persistence_size: 0,
            windows_iso: false,
            linux_iso: false,
            bypass_tpm: false,
            bypass_secureboot: false,
            bypass_ram: false,
            skip_msaccount: false,
            disable_telemetry: false,
            local_account: QString::default(),
            busy: false,
            progress: 0.0,
            phase: QString::default(),
            log_text: QString::default(),
            status: QString::from("Ready"),
            speed: QString::default(),
            eta: QString::default(),
            fit_warning: QString::default(),
            dep_warning: QString::from(&crate::deps::warning()),
            win_languages: QString::default(),
            win_options: QString::default(),
            device_list: Vec::new(),
            iso_report: None,
            win_catalog: None,
            win_option_list: Vec::new(),
            job: None,
        }
    }
}

impl qobject::AppController {
    /// Re-scan `/sys/block` for candidate target devices.
    pub fn refresh_devices(mut self: core::pin::Pin<&mut Self>) {
        let include_fixed = *self.show_fixed_disks();
        let devices = crate::devices::enumerate(include_fixed);

        let display = devices
            .iter()
            .map(DeviceInfo::display)
            .collect::<Vec<_>>()
            .join("\n");
        self.as_mut().set_devices(QString::from(&display));

        let selected = if devices.is_empty() { -1 } else { 0 };
        self.as_mut().set_selected_device(selected);
        self.as_mut().rust_mut().device_list = devices;
        self.refresh_fit_warning();
    }

    /// Select a target device by index, then refresh the capacity warning.
    pub fn select_device(mut self: core::pin::Pin<&mut Self>, index: i32) {
        self.as_mut().set_selected_device(index);
        self.refresh_fit_warning();
    }

    /// Recompute [`fit_warning`](Self::fit_warning) from the current ISO and
    /// selected device — set to a message when the image cannot possibly fit.
    fn refresh_fit_warning(mut self: core::pin::Pin<&mut Self>) {
        let iso_bytes = self
            .rust()
            .iso_report
            .as_ref()
            .map_or(0, |r| r.total_size);
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

    /// Set the source ISO (normalizing a `file://` URL) and analyze it.
    pub fn set_iso(mut self: core::pin::Pin<&mut Self>, path: &QString) {
        let raw = path.to_string();
        let path = raw.strip_prefix("file://").unwrap_or(&raw).to_string();
        if path.is_empty() {
            return;
        }
        let path_buf = PathBuf::from(&path);
        if !path_buf.is_file() {
            self.as_mut()
                .set_iso_summary(QString::from("Cannot read that file"));
            return;
        }
        let report = crate::iso::analyze(&path_buf);
        self.apply_iso(&path, report, None);
    }

    /// Set the source ISO from a just-downloaded file whose SHA-256 was
    /// already computed as it streamed — so no re-read of the ISO is needed.
    pub fn set_downloaded_iso(self: core::pin::Pin<&mut Self>, path: &str, sha256: &str) {
        let report = crate::iso::analyze(std::path::Path::new(path));
        self.apply_iso(path, report, Some(sha256));
    }

    /// Apply an analyzed ISO to the UI state. When `sha256` is `Some` the hash
    /// is already known (a downloaded ISO); otherwise it is computed off-thread.
    fn apply_iso(
        mut self: core::pin::Pin<&mut Self>,
        path: &str,
        report: IsoReport,
        sha256: Option<&str>,
    ) {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        let summary = format!("{name}  ·  {}", report.summary());
        let vol_label = report.label.clone();
        let pers_supported = report.persistence.is_some();
        let is_windows = report.os_kind == OsKind::Windows;
        let is_linux = report.os_kind == OsKind::Linux;

        self.as_mut().set_iso_path(QString::from(path));
        self.as_mut().set_iso_summary(QString::from(&summary));
        // Pre-fill the editable volume label from the image's own label.
        self.as_mut().set_label(QString::from(&vol_label));
        self.as_mut().set_persistence_supported(pers_supported);
        self.as_mut().set_persistence_size(0);
        self.as_mut().set_windows_iso(is_windows);
        self.as_mut().set_linux_iso(is_linux);

        // Auto-pick the write method the image needs: the partition method for
        // a Windows/Linux installer, raw DD for a BSD/other image (DD is
        // OS-agnostic and the only method that boots those). Leave explicit
        // "Format only" / "Ventoy" choices alone; the user can still override.
        if *self.method() < 2 {
            let auto_method = if is_windows || is_linux { 1 } else { 0 };
            self.as_mut().set_method(auto_method);
        }
        self.as_mut().rust_mut().iso_report = Some(report);

        match sha256 {
            // Downloaded ISO — the hash was computed during the download.
            Some(hash) => self.as_mut().set_iso_sha256(QString::from(hash)),
            // Local ISO — the SHA-256 of a multi-gigabyte file is slow, so
            // compute it on a worker thread without blocking the UI.
            None => {
                self.as_mut().set_iso_sha256(QString::from("Computing…"));
                let qt = self.qt_thread();
                let path = path.to_string();
                std::thread::spawn(move || crate::runner::compute_iso_sha256(qt, path));
            }
        }

        self.refresh_fit_warning();
    }

    /// Whether [`start`](Self::start) would currently do anything useful.
    pub fn can_start(&self) -> bool {
        if *self.busy() || *self.selected_device() < 0 {
            return false;
        }
        match *self.method() {
            // Format-only takes no ISO and has nothing to fit-check.
            2 => true,
            // Ventoy: an ISO is optional, but if given it must fit.
            3 => self.fit_warning().to_string().is_empty(),
            _ => {
                !self.iso_path().to_string().is_empty()
                    && self.fit_warning().to_string().is_empty()
            }
        }
    }

    /// Describe what is about to happen, for the confirmation dialog.
    pub fn confirm_text(&self) -> QString {
        let Some(dev) = self.selected_info() else {
            return QString::from("No device selected.");
        };
        if *self.method() == 3 && *self.ventoy_update() {
            return QString::from(&format!(
                "Ventoy on {} will be updated — existing files are kept.",
                dev.display()
            ));
        }
        let mut text = format!(
            "All data on {} will be permanently erased.",
            dev.display()
        );
        // Make an internal disk impossible to mistake for a USB drive.
        if !dev.removable {
            text.push_str(
                "\n\n⚠ This is an INTERNAL (non-removable) disk — \
                 make absolutely sure this is the device you mean to erase.",
            );
        }
        QString::from(&text)
    }

    /// Validate inputs, build a [`Job`], and spawn the privileged helper.
    pub fn start(mut self: core::pin::Pin<&mut Self>) {
        if !self.can_start() {
            self.as_mut()
                .set_status(QString::from("Select an ISO and a target device first"));
            return;
        }

        // Re-scan the system and confirm the chosen device still exists exactly
        // as it was enumerated. A USB drive swapped into this slot since the
        // user picked it would reuse the same `/dev` node — writing to it would
        // destroy the wrong disk. Any mismatch aborts and forces a fresh scan.
        let Some(selected) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("Select a target device first"));
            return;
        };
        let current = crate::devices::enumerate(*self.show_fixed_disks());
        if !current.contains(&selected) {
            self.as_mut().set_status(QString::from(
                "The selected device changed since it was chosen — \
                 the device list has been refreshed; check the target and start again.",
            ));
            self.as_mut().refresh_devices();
            return;
        }

        let iso = self.iso_path().to_string();
        let device = selected.path.clone();

        let table = if *self.table() == 0 {
            PartitionTable::Gpt
        } else {
            PartitionTable::Mbr
        };
        let label = self.label().to_string();
        let full_format = *self.full_format();
        let verify = *self.verify();

        let job = match *self.method() {
            0 => Job::Dd {
                iso_path: iso.into(),
                device_path: device.into(),
                opts: JobOptions::default(),
            },
            2 => Job::Format {
                device_path: device.into(),
                table,
                filesystem: filesystem_from_index(*self.filesystem()),
                opts: JobOptions {
                    label,
                    full_format,
                    verify,
                },
            },
            3 => Job::Ventoy {
                device_path: device.into(),
                table,
                secure_boot: *self.ventoy_secure_boot(),
                update: *self.ventoy_update(),
                // Seed the Ventoy partition with the loaded ISO, if any.
                iso_path: (!iso.is_empty()).then(|| iso.into()),
            },
            _ => {
                // Filesystem and large-`install.wim` handling are decided
                // automatically from the ISO analysis: NTFS + UEFI:NTFS for a
                // Windows ISO with an oversized install.wim, FAT32 otherwise.
                let (filesystem, wim) = self
                    .rust()
                    .iso_report
                    .as_ref()
                    .map(usbooty_core::auto_filesystem)
                    .unwrap_or((FileSystem::Fat32, WimStrategy::None));
                // A persistent overlay, when the ISO supports it and the user
                // gave the slider a non-zero size.
                let persistence = self
                    .rust()
                    .iso_report
                    .as_ref()
                    .and_then(|r| r.persistence)
                    .filter(|_| *self.persistence_size() > 0)
                    .map(|kind| Persistence {
                        kind,
                        size_bytes: *self.persistence_size() as u64 * 1024 * 1024,
                    });
                // Windows-installer customization, when the source is Windows.
                let windows_setup = if *self.windows_iso() {
                    let setup = WindowsSetup {
                        bypass_tpm: *self.bypass_tpm(),
                        bypass_secureboot: *self.bypass_secureboot(),
                        bypass_ram: *self.bypass_ram(),
                        skip_msaccount: *self.skip_msaccount(),
                        disable_telemetry: *self.disable_telemetry(),
                        local_account: {
                            let name = self.local_account().to_string();
                            (!name.trim().is_empty()).then(|| name.trim().to_string())
                        },
                    };
                    setup.is_active().then_some(setup)
                } else {
                    None
                };
                Job::Partitioned {
                    iso_path: iso.into(),
                    device_path: device.into(),
                    table,
                    filesystem,
                    wim,
                    // The runner downloads and fills this in when needed.
                    uefi_ntfs_img: None,
                    persistence,
                    windows_setup,
                    opts: JobOptions {
                        label,
                        full_format,
                        verify,
                    },
                }
            }
        };

        self.as_mut().set_busy(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_phase(QString::from("Starting"));
        self.as_mut().set_log_text(QString::default());
        self.as_mut().set_speed(QString::default());
        self.as_mut().set_eta(QString::default());
        self.as_mut().set_status(QString::from("Running…"));

        let stdin_slot: Arc<Mutex<Option<std::process::ChildStdin>>> = Arc::new(Mutex::new(None));
        let handle = JobHandle {
            stdin: stdin_slot.clone(),
        };
        let qt_thread = self.qt_thread();

        std::thread::spawn(move || {
            crate::runner::run_job(job, qt_thread, stdin_slot);
        });

        self.as_mut().rust_mut().job = Some(handle);
    }

    /// Fetch the language list for a Windows release (an index into
    /// [`crate::windisco::RELEASES`]).
    pub fn win_fetch_languages(mut self: core::pin::Pin<&mut Self>, version_index: i32) {
        if *self.busy() {
            return;
        }
        let Some(&(_, edition_id)) = crate::windisco::RELEASES.get(version_index.max(0) as usize)
        else {
            return;
        };
        self.as_mut().set_busy(true);
        self.as_mut().set_win_languages(QString::default());
        self.as_mut().set_win_options(QString::default());
        self.as_mut()
            .set_status(QString::from("Contacting Microsoft…"));

        let qt = self.qt_thread();
        std::thread::spawn(move || crate::runner::win_fetch_languages(qt, edition_id));
    }

    /// Fetch the download options for a previously-listed language.
    pub fn win_fetch_options(mut self: core::pin::Pin<&mut Self>, language_index: i32) {
        if *self.busy() || language_index < 0 {
            return;
        }
        let Some(catalog) = self.rust().win_catalog.clone() else {
            return;
        };
        self.as_mut().set_busy(true);
        self.as_mut().set_win_options(QString::default());
        self.as_mut()
            .set_status(QString::from("Fetching download options…"));

        let qt = self.qt_thread();
        std::thread::spawn(move || {
            crate::runner::win_fetch_options(qt, catalog, language_index as usize)
        });
    }

    /// Download a previously-listed Windows ISO option and select it.
    pub fn win_download(mut self: core::pin::Pin<&mut Self>, option_index: i32) {
        if *self.busy() || option_index < 0 {
            return;
        }
        let Some(option) = self.rust().win_option_list.get(option_index as usize) else {
            return;
        };
        let url = option.url.clone();
        self.as_mut().set_busy(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_log_text(QString::default());
        self.as_mut().set_phase(QString::from("Starting"));
        self.as_mut().set_speed(QString::default());
        self.as_mut().set_eta(QString::default());
        self.as_mut()
            .set_status(QString::from("Downloading Windows ISO…"));

        let qt = self.qt_thread();
        std::thread::spawn(move || crate::runner::download_windows_url(qt, url));
    }

    /// Open Microsoft's official download page in the system browser — the
    /// reliable fallback when Microsoft's anti-bot system blocks the in-app
    /// query (common on VPNs and some ISPs).
    pub fn open_microsoft_page(&self, version_index: i32) {
        let url = if version_index == 1 {
            "https://www.microsoft.com/software-download/windows10"
        } else {
            "https://www.microsoft.com/software-download/windows11"
        };
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }

    /// Ask the running helper to abort by writing `cancel` to its stdin.
    pub fn cancel(mut self: core::pin::Pin<&mut Self>) {
        if let Some(job) = &self.rust().job {
            if let Ok(mut guard) = job.stdin.lock() {
                if let Some(stdin) = guard.as_mut() {
                    use std::io::Write;
                    let _ = writeln!(stdin, "cancel");
                    let _ = stdin.flush();
                }
            }
        }
        self.as_mut().set_status(QString::from("Cancelling…"));
    }

    /// The [`DeviceInfo`] for the current `selected_device` index, if valid.
    fn selected_info(&self) -> Option<&DeviceInfo> {
        let idx = *self.selected_device();
        if idx < 0 {
            None
        } else {
            self.rust().device_list.get(idx as usize)
        }
    }
}

/// Map a GUI filesystem-combo index to a [`FileSystem`].
fn filesystem_from_index(index: i32) -> FileSystem {
    match index {
        1 => FileSystem::Ntfs,
        2 => FileSystem::ExFat,
        3 => FileSystem::Ext4,
        _ => FileSystem::Fat32,
    }
}
