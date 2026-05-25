//! The `AppController` QObject — the single object QML binds to.
//!
//! Properties hold all UI state; invokables are the actions QML triggers. The
//! heavy lifting (device enumeration, running the privileged helper) lives in
//! sibling modules; this file is the Qt-facing surface.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::{
    CheckMode, DeviceInfo, FileSystem, IsoReport, Job, JobOptions, OsKind, PartitionTable,
    Persistence, WimStrategy, WindowsSetup,
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
        // Digests of the source ISO ("Computing…" while they are calculated).
        // SHA-256 is the most commonly published, kept first for back-compat.
        #[qproperty(QString, iso_sha256)]
        #[qproperty(QString, iso_md5)]
        #[qproperty(QString, iso_sha1)]
        #[qproperty(QString, iso_sha512)]
        #[qproperty(QString, iso_blake3)]
        // 0.0..=1.0 while `compute_hashes` runs; the UI binds to it to show
        // a percentage instead of a frozen "Computing…" placeholder.
        #[qproperty(f64, hash_progress)]
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
        // For Windows ISOs with install.wim larger than 4 GiB, choose between
        // UEFI:NTFS (false, default) and wimlib-imagex split onto FAT32 (true).
        #[qproperty(bool, split_wim)]
        #[qproperty(bool, bypass_tpm)]
        #[qproperty(bool, bypass_secureboot)]
        #[qproperty(bool, bypass_ram)]
        #[qproperty(bool, bypass_storage)]
        #[qproperty(bool, bypass_cpu)]
        #[qproperty(bool, bypass_disk)]
        #[qproperty(bool, skip_msaccount)]
        #[qproperty(bool, disable_network_during_oobe)]
        #[qproperty(bool, hide_wireless_setup)]
        #[qproperty(bool, hide_oem_registration)]
        #[qproperty(bool, network_location_work)]
        #[qproperty(bool, disable_telemetry)]
        #[qproperty(bool, accept_eula)]
        #[qproperty(bool, enable_dotnet35)]
        #[qproperty(bool, apply_debloat)]
        #[qproperty(QString, local_account)]
        #[qproperty(QString, local_account_password)]
        #[qproperty(QString, computer_name)]
        #[qproperty(QString, locale)]
        #[qproperty(QString, timezone)]
        #[qproperty(QString, product_key)]
        // Read-only catalogs for the Windows-setup dialog's time-zone picker:
        // parallel lists of friendly labels and Microsoft TimeZone IDs.
        #[qproperty(QString, timezone_labels)]
        #[qproperty(QString, timezone_ids)]
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
        // Newline-joined warnings raised by the SBAT/DBX revocation scan of
        // the ISO's signed EFI binaries; empty when no issue was found.
        #[qproperty(QString, revocation_warnings)]
        // Short warning from a background SMART probe of the selected device
        // (reallocated sectors, temperature warnings, failing prediction);
        // empty when the device looks healthy or smartmontools isn't installed.
        #[qproperty(QString, smart_warning)]
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
        /// Forget the currently-loaded source ISO and reset every field
        /// derived from it (path, summary, label, OS chip, digests,
        /// persistence support, revocation warnings, fit warning).
        #[qinvokable]
        fn clear_iso(self: Pin<&mut AppController>);
        /// Validate inputs and launch the privileged helper.
        #[qinvokable]
        fn start(self: Pin<&mut AppController>);
        /// Request cancellation of the running job.
        #[qinvokable]
        fn cancel(self: Pin<&mut AppController>);
        /// Whether a confirmation dialog should be shown before starting.
        #[qinvokable]
        fn can_start(self: &AppController) -> bool;
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
        /// Compute every digest (MD5/SHA-1/SHA-256/SHA-512/BLAKE3) of the
        /// currently-loaded source ISO on a worker thread. CPU-heavy and
        /// disk-bound, so the GUI only runs it on demand.
        #[qinvokable]
        fn compute_hashes(self: Pin<&mut AppController>);
        /// Read the selected device into the given image file (a snapshot /
        /// backup — the inverse of writing). The output is compressed when
        /// the path ends in `.gz`, `.xz`, `.zst`, or `.bz2`.
        #[qinvokable]
        fn start_backup(self: Pin<&mut AppController>, image_path: &QString);
        /// Run an integrity check on the selected device: `mode_index == 0`
        /// is the fast F3 fake-capacity check, `1` is the full bad-blocks scan.
        #[qinvokable]
        fn start_check(self: Pin<&mut AppController>, mode_index: i32);
        /// Eject (power-off) the currently-selected device using `udisksctl`
        /// when available, falling back to `eject -F`. Best-effort; logs the
        /// outcome via `set_status` and clears the device selection on success.
        #[qinvokable]
        fn eject_device(self: Pin<&mut AppController>);

        // ---- Structured accessors for the selected device, kept separate
        //      so the confirm dialog can lay them out visually instead of
        //      crammed into a single line of `confirm_text`.
        #[qinvokable]
        fn selected_model(self: &AppController) -> QString;
        #[qinvokable]
        fn selected_size_text(self: &AppController) -> QString;
        #[qinvokable]
        fn selected_path(self: &AppController) -> QString;
        #[qinvokable]
        fn selected_is_internal(self: &AppController) -> bool;
        #[qinvokable]
        fn selected_bus(self: &AppController) -> QString;
        #[qinvokable]
        fn selected_serial(self: &AppController) -> QString;
        /// Largest persistence size (in MiB) that still leaves room for the
        /// ISO + a small partition-table margin on the selected device.
        /// Returns 0 when the slider should stay at zero.
        #[qinvokable]
        fn max_persistence_mib(self: &AppController) -> i32;
        /// The current label trimmed/sanitized to the limits of the chosen
        /// filesystem (FAT32 → 11 chars upper, NTFS → 32 chars, exFAT → 11
        /// chars, ext4 → 16 chars). Used as a tooltip preview on the field.
        #[qinvokable]
        fn sanitized_label(self: &AppController) -> QString;
        /// Combined `lsblk -O` and `udevadm info` dump for the selected
        /// device, for the confirm dialog's "Inspect" panel.
        #[qinvokable]
        fn inspect_selected(self: &AppController) -> QString;
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
///
/// Helper-driven jobs (DD, partitioned, format, backup, check) cancel by
/// writing `cancel` to the helper's stdin. The Windows-ISO download runs in
/// a plain worker thread instead, with no helper, so it uses an atomic flag
/// that the download loop polls.
pub struct JobHandle {
    /// The helper's stdin — writing `cancel` here aborts it.
    pub stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
    /// Cancellation flag for the Windows-ISO download.
    pub download_abort: Option<Arc<AtomicBool>>,
}

/// Backing storage for [`qobject::AppController`].
pub struct AppControllerRust {
    iso_path: QString,
    iso_summary: QString,
    label: QString,
    iso_sha256: QString,
    iso_md5: QString,
    iso_sha1: QString,
    iso_sha512: QString,
    iso_blake3: QString,
    hash_progress: f64,
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
    split_wim: bool,
    bypass_tpm: bool,
    bypass_secureboot: bool,
    bypass_ram: bool,
    bypass_storage: bool,
    bypass_cpu: bool,
    bypass_disk: bool,
    skip_msaccount: bool,
    disable_network_during_oobe: bool,
    hide_wireless_setup: bool,
    hide_oem_registration: bool,
    network_location_work: bool,
    disable_telemetry: bool,
    accept_eula: bool,
    enable_dotnet35: bool,
    apply_debloat: bool,
    local_account: QString,
    local_account_password: QString,
    computer_name: QString,
    locale: QString,
    timezone: QString,
    product_key: QString,
    timezone_labels: QString,
    timezone_ids: QString,
    busy: bool,
    progress: f64,
    phase: QString,
    log_text: QString,
    status: QString,
    speed: QString,
    eta: QString,
    fit_warning: QString,
    revocation_warnings: QString,
    smart_warning: QString,
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
            iso_md5: QString::default(),
            iso_sha1: QString::default(),
            iso_sha512: QString::default(),
            iso_blake3: QString::default(),
            hash_progress: 0.0,
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
            split_wim: false,
            bypass_tpm: false,
            bypass_secureboot: false,
            bypass_ram: false,
            bypass_storage: false,
            bypass_cpu: false,
            bypass_disk: false,
            skip_msaccount: false,
            disable_network_during_oobe: false,
            hide_wireless_setup: false,
            hide_oem_registration: false,
            network_location_work: false,
            disable_telemetry: false,
            accept_eula: false,
            enable_dotnet35: false,
            apply_debloat: false,
            local_account: QString::default(),
            local_account_password: QString::default(),
            computer_name: QString::default(),
            locale: QString::default(),
            timezone: QString::default(),
            product_key: QString::default(),
            timezone_labels: QString::from(&crate::timezones::labels()),
            timezone_ids: QString::from(&crate::timezones::ids()),
            busy: false,
            progress: 0.0,
            phase: QString::default(),
            log_text: QString::default(),
            status: QString::from("Ready"),
            speed: QString::default(),
            eta: QString::default(),
            fit_warning: QString::default(),
            revocation_warnings: QString::default(),
            smart_warning: QString::default(),
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

    /// Select a target device by index, then refresh the capacity warning
    /// and kick off a background SMART probe of the chosen device.
    pub fn select_device(mut self: core::pin::Pin<&mut Self>, index: i32) {
        self.as_mut().set_selected_device(index);
        self.as_mut().set_smart_warning(QString::default());
        self.as_mut().refresh_fit_warning();
        self.as_mut().probe_smart();
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
                    ctrl.as_mut().set_smart_warning(QString::from(&warning));
                },
            );
        });
    }

    /// Recompute [`fit_warning`](Self::fit_warning) from the current ISO and
    /// selected device — set to a message when the image cannot possibly fit.
    fn refresh_fit_warning(mut self: core::pin::Pin<&mut Self>) {
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

    /// Reset every field derived from the source ISO so the slot looks
    /// "fresh" again. Used by the *Clear source image* menu entry; the
    /// inverse of `apply_iso`.
    pub fn clear_iso(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().set_iso_path(QString::default());
        self.as_mut()
            .set_iso_summary(QString::from("No image selected"));
        self.as_mut().set_label(QString::default());
        self.as_mut().set_windows_iso(false);
        self.as_mut().set_linux_iso(false);
        self.as_mut().set_persistence_supported(false);
        self.as_mut().set_persistence_size(0);
        self.as_mut().set_iso_md5(QString::default());
        self.as_mut().set_iso_sha1(QString::default());
        self.as_mut().set_iso_sha256(QString::default());
        self.as_mut().set_iso_sha512(QString::default());
        self.as_mut().set_iso_blake3(QString::default());
        self.as_mut().set_hash_progress(0.0);
        self.as_mut().set_revocation_warnings(QString::default());
        self.as_mut().rust_mut().iso_report = None;
        self.refresh_fit_warning();
    }

    /// Set the source ISO from a just-downloaded file whose digests were
    /// already computed as it streamed, so no re-read of the ISO is needed.
    pub fn set_downloaded_iso(
        self: core::pin::Pin<&mut Self>,
        path: &str,
        hashes: &crate::iso::IsoHashes,
    ) {
        let report = crate::iso::analyze(std::path::Path::new(path));
        self.apply_iso(path, report, Some(hashes));
    }

    /// Apply an analyzed ISO to the UI state. When `hashes` is `Some` the
    /// digests are already known (a downloaded ISO); otherwise they are
    /// computed off-thread.
    fn apply_iso(
        mut self: core::pin::Pin<&mut Self>,
        path: &str,
        report: IsoReport,
        hashes: Option<&crate::iso::IsoHashes>,
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
        let rev_text = report.revocation_warnings.join("\n");
        self.as_mut()
            .set_revocation_warnings(QString::from(&rev_text));
        self.as_mut().rust_mut().iso_report = Some(report);

        match hashes {
            // Downloaded ISO — every digest was computed during the download.
            Some(h) => {
                self.as_mut().set_iso_md5(QString::from(&h.md5));
                self.as_mut().set_iso_sha1(QString::from(&h.sha1));
                self.as_mut().set_iso_sha256(QString::from(&h.sha256));
                self.as_mut().set_iso_sha512(QString::from(&h.sha512));
                self.as_mut().set_iso_blake3(QString::from(&h.blake3));
            }
            // Local ISO — hashing a multi-gigabyte file is CPU-heavy (five
            // hashers updated per chunk), so leave the digests blank until
            // the user explicitly asks for them via `compute_hashes()`.
            None => {
                self.as_mut().set_iso_md5(QString::default());
                self.as_mut().set_iso_sha1(QString::default());
                self.as_mut().set_iso_sha256(QString::default());
                self.as_mut().set_iso_sha512(QString::default());
                self.as_mut().set_iso_blake3(QString::default());
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
                !self.iso_path().to_string().is_empty() && self.fit_warning().to_string().is_empty()
            }
        }
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

        // Pre-flight: the GUI runs as the user, so we can't open the device
        // for writing here — that's the helper's job, and it's run with sudo.
        // We *can* read /proc/mounts to catch the most common avoidable
        // failure (a partition still mounted from a file manager), which the
        // helper would otherwise have to fight through with `unmount_all`.
        if let Some(mountpoint) = is_device_mounted(&selected.path) {
            self.as_mut().set_status(QString::from(&format!(
                "A partition of {} is still mounted at {mountpoint}. \
                 Unmount it (and close any file manager that has it open) and try again.",
                selected.path,
            )));
            return;
        }
        if !std::path::Path::new(&selected.path).exists() {
            self.as_mut().set_status(QString::from(&format!(
                "{} no longer exists — was the drive removed?",
                selected.path,
            )));
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
                // When the user chose `Split`, override the layout to FAT32
                // and let `wimsplit` chunk install.wim after the copy.
                let (mut filesystem, mut wim) = self
                    .rust()
                    .iso_report
                    .as_ref()
                    .map(usbooty_core::auto_filesystem)
                    .unwrap_or((FileSystem::Fat32, WimStrategy::None));
                if *self.split_wim() && wim == WimStrategy::UefiNtfs {
                    filesystem = FileSystem::Fat32;
                    wim = WimStrategy::Split;
                }
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
                        bypass_storage: *self.bypass_storage(),
                        bypass_cpu: *self.bypass_cpu(),
                        bypass_disk: *self.bypass_disk(),
                        skip_msaccount: *self.skip_msaccount(),
                        disable_network_during_oobe: *self.disable_network_during_oobe(),
                        hide_wireless_setup: *self.hide_wireless_setup(),
                        hide_oem_registration: *self.hide_oem_registration(),
                        network_location_work: *self.network_location_work(),
                        disable_telemetry: *self.disable_telemetry(),
                        accept_eula: *self.accept_eula(),
                        enable_dotnet35: *self.enable_dotnet35(),
                        apply_debloat: *self.apply_debloat(),
                        local_account: trimmed_opt(&self.local_account().to_string()),
                        local_account_password: non_empty_opt(
                            &self.local_account_password().to_string(),
                        ),
                        computer_name: trimmed_opt(&self.computer_name().to_string()),
                        locale: trimmed_opt(&self.locale().to_string()),
                        timezone: trimmed_opt(&self.timezone().to_string()),
                        product_key: trimmed_opt(&self.product_key().to_string()),
                    };
                    setup.is_active().then_some(setup)
                } else {
                    None
                };
                // Offer the legacy-BIOS Syslinux/extlinux installer for Linux
                // ISOs that ship an isolinux config — Windows ISOs already
                // come with their own boot loader, so skip them.
                let install_bootloader = *self.linux_iso()
                    && self
                        .rust()
                        .iso_report
                        .as_ref()
                        .is_some_and(|r| r.has_isolinux);
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
                    install_bootloader,
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
            download_abort: None,
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
        let abort = Arc::new(AtomicBool::new(false));
        let abort_clone = abort.clone();
        std::thread::spawn(move || crate::runner::download_windows_url(qt, url, abort_clone));

        // Park a JobHandle so cancel() can reach the download — the
        // stdin slot stays empty because there is no helper to talk to.
        self.as_mut().rust_mut().job = Some(JobHandle {
            stdin: Arc::new(Mutex::new(None)),
            download_abort: Some(abort),
        });
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

    /// Kick off off-thread digest computation for the currently-loaded ISO.
    /// Sets every digest field to a "Computing…" placeholder while the work
    /// runs and fills them in as soon as it finishes. `hash_progress` is
    /// updated as fractions of completion so the UI can show a percent.
    pub fn compute_hashes(mut self: core::pin::Pin<&mut Self>) {
        let path = self.iso_path().to_string();
        if path.is_empty() {
            return;
        }
        let placeholder = QString::from("Computing…");
        self.as_mut().set_iso_md5(placeholder.clone());
        self.as_mut().set_iso_sha1(placeholder.clone());
        self.as_mut().set_iso_sha256(placeholder.clone());
        self.as_mut().set_iso_sha512(placeholder.clone());
        self.as_mut().set_iso_blake3(placeholder);
        self.as_mut().set_hash_progress(0.0);

        let qt = self.qt_thread();
        std::thread::spawn(move || crate::runner::compute_iso_hashes(qt, path));
    }

    /// Build a [`Job::Check`] for the currently-selected device and run it.
    pub fn start_check(mut self: core::pin::Pin<&mut Self>, mode_index: i32) {
        if *self.busy() {
            return;
        }
        let Some(selected) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("Select a target device first"));
            return;
        };
        let mode = if mode_index == 1 {
            CheckMode::Full
        } else {
            CheckMode::Quick
        };

        let job = Job::Check {
            device_path: selected.path.clone().into(),
            mode,
        };

        self.as_mut().set_busy(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_phase(QString::from("Starting"));
        self.as_mut().set_log_text(QString::default());
        self.as_mut().set_speed(QString::default());
        self.as_mut().set_eta(QString::default());
        self.as_mut().set_status(QString::from("Checking device…"));

        let stdin_slot: Arc<Mutex<Option<std::process::ChildStdin>>> = Arc::new(Mutex::new(None));
        let handle = JobHandle {
            stdin: stdin_slot.clone(),
            download_abort: None,
        };
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            crate::runner::run_job(job, qt_thread, stdin_slot);
        });
        self.as_mut().rust_mut().job = Some(handle);
    }

    /// Build a [`Job::Backup`] for the currently-selected device and run it.
    pub fn start_backup(mut self: core::pin::Pin<&mut Self>, image_path: &QString) {
        if *self.busy() {
            return;
        }
        let Some(selected) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("Select a target device first"));
            return;
        };
        let raw = image_path.to_string();
        let path = raw.strip_prefix("file://").unwrap_or(&raw).to_string();
        if path.is_empty() {
            self.as_mut()
                .set_status(QString::from("Pick an output file for the backup"));
            return;
        }

        let job = Job::Backup {
            device_path: selected.path.clone().into(),
            image_path: path.into(),
            opts: JobOptions {
                label: String::new(),
                full_format: false,
                verify: *self.verify(),
            },
        };

        self.as_mut().set_busy(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_phase(QString::from("Starting"));
        self.as_mut().set_log_text(QString::default());
        self.as_mut().set_speed(QString::default());
        self.as_mut().set_eta(QString::default());
        self.as_mut().set_status(QString::from("Backing up…"));

        let stdin_slot: Arc<Mutex<Option<std::process::ChildStdin>>> = Arc::new(Mutex::new(None));
        let handle = JobHandle {
            stdin: stdin_slot.clone(),
            download_abort: None,
        };
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            crate::runner::run_job(job, qt_thread, stdin_slot);
        });
        self.as_mut().rust_mut().job = Some(handle);
    }

    /// Ask the running job to abort. Helper-driven jobs hear about it through
    /// a `cancel` line on the helper's stdin; the Windows-ISO downloader
    /// polls an atomic flag instead, so flip both.
    pub fn cancel(mut self: core::pin::Pin<&mut Self>) {
        if let Some(job) = &self.rust().job {
            if let Ok(mut guard) = job.stdin.lock() {
                if let Some(stdin) = guard.as_mut() {
                    use std::io::Write;
                    let _ = writeln!(stdin, "cancel");
                    let _ = stdin.flush();
                }
            }
            if let Some(abort) = &job.download_abort {
                abort.store(true, std::sync::atomic::Ordering::SeqCst);
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
    pub fn max_persistence_mib(&self) -> i32 {
        let Some(device) = self.selected_info() else {
            return 0;
        };
        let iso_size = self
            .rust()
            .iso_report
            .as_ref()
            .map_or(0u64, |r| r.total_size);
        const MARGIN: u64 = 64 * 1024 * 1024;
        let usable = device.size.saturating_sub(iso_size).saturating_sub(MARGIN);
        let mib = usable / (1024 * 1024);
        i32::try_from(mib).unwrap_or(i32::MAX)
    }

    /// Trim the current label down to whatever fits on the chosen filesystem,
    /// matching what the helper will end up writing. Pure preview, no state
    /// change — surfaced as a tooltip on the volume-label field.
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

    /// `lsblk` + `udevadm info` for the selected device, joined into one
    /// human-readable dump. Returns an empty string when no device is
    /// selected. Blocking — the dialog opens once it returns.
    pub fn inspect_selected(&self) -> QString {
        let Some(device) = self.selected_info() else {
            return QString::default();
        };
        let path = device.path.clone();
        // lsblk: passing both `-O` (all columns) and `--output` blanks the
        // output on most versions, so pick a useful column set explicitly.
        // Dropping `-d` keeps the disk + its partitions, which is exactly
        // what the user wants to see before erasing.
        let lsblk = std::process::Command::new("lsblk")
            .args([
                "-p",
                "--output",
                "NAME,SIZE,TYPE,FSTYPE,LABEL,UUID,PARTLABEL,MOUNTPOINTS,MODEL,VENDOR,TRAN,REV,ROTA,RM,RO,HOTPLUG",
            ])
            .arg(&path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_else(|e| format!("(lsblk failed: {e})"));
        let udev_raw = std::process::Command::new("udevadm")
            .args(["info", "--query=property", "--name"])
            .arg(&path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_else(|e| format!("(udevadm failed: {e})"));
        let udev = clean_udev(&udev_raw);
        // smartctl: info + overall health + attribute table is the
        // "is this drive ok?" subset. Self-test and error logs would bloat
        // the panel for no everyday benefit. Exit code is non-zero whenever
        // SMART is unsupported or permission is denied (both common), so
        // the panel inspects stderr to give a useful message either way.
        let smart = match std::process::Command::new("smartctl")
            .args(["-i", "-H", "-A"])
            .arg(&path)
            .output()
        {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let combined = stdout.trim().to_string();
                if combined.contains("Permission denied") || stderr.contains("Permission denied") {
                    "(smartctl needs root for raw device access — either run\n \
                       sudo chmod u+s $(which smartctl)\n \
                     once to setuid the binary, or launch usbooty with sudo.)"
                        .to_string()
                } else if combined.is_empty() {
                    let tail = stderr.trim();
                    if tail.is_empty() {
                        "(smartctl returned no output — device may not expose SMART)".to_string()
                    } else {
                        format!("(smartctl: {tail})")
                    }
                } else {
                    combined
                }
            }
            Err(_) => "(smartctl not installed — install the `smartmontools` package \
                       to see SMART health here)"
                .to_string(),
        };
        let combined = format!(
            "── lsblk ───────────────────────────────────────────\n{lsblk}\n\
             ── udevadm ─────────────────────────────────────────\n{udev}\n\
             ── smartctl ────────────────────────────────────────\n{smart}"
        );
        QString::from(&combined)
    }

    /// Try to power off the currently-selected USB device. Best-effort: prefers
    /// `udisksctl power-off` (the desktop standard, handles unmount + safe
    /// removal in one call), falling back to `eject -F`. Either tool runs as
    /// the user — no helper hop needed. The selection is cleared and the
    /// device list refreshed on success so the now-detached device disappears
    /// from the combo.
    pub fn eject_device(mut self: core::pin::Pin<&mut Self>) {
        let Some(device) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("No device selected"));
            return;
        };
        let path = device.path.clone();
        let result = std::process::Command::new("udisksctl")
            .args(["power-off", "-b", &path])
            .output()
            .or_else(|_| {
                std::process::Command::new("eject")
                    .args(["-F", &path])
                    .output()
            });
        match result {
            Ok(out) if out.status.success() => {
                self.as_mut()
                    .set_status(QString::from(&format!("Ejected {path}")));
                self.refresh_devices();
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                self.as_mut()
                    .set_status(QString::from(&format!("Eject failed: {}", err.trim())));
            }
            Err(e) => {
                self.as_mut()
                    .set_status(QString::from(&format!("Eject failed: {e}")));
            }
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

/// `Some(trimmed)` when `s` has any non-whitespace content; `None` otherwise.
///
/// The unattend generator treats `Some("")` the same as `None`, but using
/// `None` keeps the resulting JSON tidy and round-trips cleanly.
fn trimmed_opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// `Some(s)` when `s` is non-empty *without* trimming — passwords keep their
/// leading and trailing whitespace because Windows compares them exactly.
fn non_empty_opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// Strip noisy duplicates from a `udevadm info --query=property` dump:
/// every `ID_FOO_ENC=…` is the same value as `ID_FOO=…` with spaces and
/// other bytes hex-encoded, so removing them roughly halves the output
/// without losing any information.
fn clean_udev(raw: &str) -> String {
    raw.lines()
        .filter(|line| {
            let key = line.split('=').next().unwrap_or("");
            !key.ends_with("_ENC")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return the mount-point of the first mounted partition of `device_path`,
/// or `None` when nothing on the device is mounted. Reads `/proc/mounts`
/// directly — no root required and no race that matters at this resolution
/// (the helper will re-check with `umount` before writing).
///
/// Matches `/dev/sdc` against `/dev/sdc`, `/dev/sdc1`, `/dev/sdc2`, … by
/// checking the prefix and then that the next char is either nothing or a
/// digit. NVMe partitions (`nvme0n1p1`) work the same way.
fn is_device_mounted(device_path: &str) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let source = parts.next()?;
        let target = parts.next()?;
        if source == device_path {
            return Some(target.to_string());
        }
        if let Some(tail) = source.strip_prefix(device_path) {
            // /dev/sdc1, /dev/sdc12 — partition number suffix.
            // /dev/nvme0n1p1 — `p` then digits.
            let first = tail.chars().next();
            if first.is_some_and(|c| c.is_ascii_digit())
                || tail
                    .strip_prefix('p')
                    .is_some_and(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))
            {
                return Some(target.to_string());
            }
        }
    }
    None
}
