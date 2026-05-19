//! The `AppController` QObject — the single object QML binds to.
//!
//! Properties hold all UI state; invokables are the actions QML triggers. The
//! heavy lifting (device enumeration, running the privileged helper) lives in
//! sibling modules; this file is the Qt-facing surface.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::{DeviceInfo, IsoReport, Job, PartitionTable, WimStrategy};

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
        // Target devices: newline-separated display strings for the combo box.
        #[qproperty(QString, devices)]
        #[qproperty(i32, selected_device)]
        #[qproperty(bool, show_fixed_disks)]
        // Options. method: 0 = DD, 1 = partition & copy. table: 0 = GPT, 1 = MBR.
        #[qproperty(i32, method)]
        #[qproperty(i32, table)]
        // Large-install.wim handling, set by the choice dialog:
        // 0 = split install.wim, 1 = UEFI:NTFS two-partition layout.
        #[qproperty(i32, wim_choice)]
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
        /// Whether the FAT32 method must ask how to handle a large install.wim.
        #[qinvokable]
        fn needs_wim_choice(self: &AppController) -> bool;
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
    devices: QString,
    selected_device: i32,
    show_fixed_disks: bool,
    method: i32,
    table: i32,
    wim_choice: i32,
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
            devices: QString::default(),
            selected_device: -1,
            show_fixed_disks: false,
            method: 0,
            table: 0,
            wim_choice: 0,
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
        let name = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let summary = format!("{name}  ·  {}", report.summary());

        self.as_mut().set_iso_path(QString::from(&path));
        self.as_mut().set_iso_summary(QString::from(&summary));
        self.as_mut().rust_mut().iso_report = Some(report);
        self.refresh_fit_warning();
    }

    /// Whether the FAT32 method must prompt for large-`install.wim` handling.
    pub fn needs_wim_choice(&self) -> bool {
        *self.method() == 1
            && self
                .rust()
                .iso_report
                .as_ref()
                .is_some_and(usbooty_core::needs_wim_choice)
    }

    /// Whether [`start`](Self::start) would currently do anything useful.
    pub fn can_start(&self) -> bool {
        !self.busy()
            && !self.iso_path().to_string().is_empty()
            && *self.selected_device() >= 0
            && self.fit_warning().to_string().is_empty()
    }

    /// Describe the device about to be erased, for the confirmation dialog.
    pub fn confirm_text(&self) -> QString {
        match self.selected_info() {
            Some(dev) => QString::from(&format!(
                "All data on {} will be permanently erased.",
                dev.display()
            )),
            None => QString::from("No device selected."),
        }
    }

    /// Validate inputs, build a [`Job`], and spawn the privileged helper.
    pub fn start(mut self: core::pin::Pin<&mut Self>) {
        if !self.can_start() {
            self.as_mut()
                .set_status(QString::from("Select an ISO and a target device first"));
            return;
        }

        let iso = self.iso_path().to_string();
        let Some(device) = self.selected_info().map(|d| d.path.clone()) else {
            return;
        };

        let job = if *self.method() == 0 {
            Job::Dd {
                iso_path: iso.into(),
                device_path: device.into(),
            }
        } else {
            let table = if *self.table() == 0 {
                PartitionTable::Gpt
            } else {
                PartitionTable::Mbr
            };
            // The user's answer to the large-install.wim prompt; `choose_scheme`
            // ignores it unless the ISO actually needs the choice.
            let user_wim = if *self.wim_choice() == 1 {
                WimStrategy::UefiNtfs
            } else {
                WimStrategy::Split
            };
            let wim_strategy = self
                .rust()
                .iso_report
                .as_ref()
                .map(|report| {
                    usbooty_core::choose_scheme(report, table, Some(user_wim)).wim_strategy
                })
                .unwrap_or(WimStrategy::None);
            // Name the partition after the source image's own volume label.
            let label = self
                .rust()
                .iso_report
                .as_ref()
                .map(|report| report.label.clone())
                .unwrap_or_default();
            Job::Partitioned {
                iso_path: iso.into(),
                device_path: device.into(),
                table,
                wim_strategy,
                // The runner downloads and fills this in when needed.
                uefi_ntfs_img: None,
                label,
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
