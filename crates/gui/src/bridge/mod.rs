//! The `AppController` QObject: the single object QML binds to.
//!
//! Properties hold all UI state; invokables are the actions QML triggers. The
//! heavy lifting (device enumeration, running the privileged helper) lives in
//! sibling modules; this file is the Qt-facing surface.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

mod app;
mod devices;
mod helpers;
mod iso;
mod jobs;
mod state;
mod windows;

use state::AppControllerRust;

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
        // Digests of the source ISO. Empty until computed; each fills in as
        // its hash worker finishes. SHA-256 is the most commonly published,
        // kept first for back-compat.
        #[qproperty(QString, iso_sha256)]
        #[qproperty(QString, iso_md5)]
        #[qproperty(QString, iso_sha1)]
        #[qproperty(QString, iso_sha512)]
        #[qproperty(QString, iso_blake3)]
        // 0.0..=1.0 while `compute_hashes` runs; the UI binds to it for the
        // percentage shown next to the per-hash spinners.
        #[qproperty(f64, hash_progress)]
        // True while digests are being computed; the panel shows a spinner
        // per hash until each value lands.
        #[qproperty(bool, hashing)]
        // Cross-check result from the rg-adguard SHA-1 database: when the
        // computed SHA-1 matches a known retail Microsoft ISO, the upstream
        // service returns a canonical filename + category which the UI
        // surfaces as a green "verified" badge. Empty when no match (the
        // common case for non-Windows ISOs and for unrecognised hashes).
        #[qproperty(QString, iso_adguard_badge)]
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
        // True when persistence lives in a folder on the data partition
        // rather than a dedicated partition (currently only Slax). QML
        // swaps the size slider for a simple on/off checkbox in that case.
        #[qproperty(bool, persistence_inline)]
        #[qproperty(i32, persistence_size)]
        // Maximum slider value in MiB, derived from the *currently selected*
        // device's free space minus the ISO and a small filesystem margin.
        // Updates reactively whenever the device or the ISO changes, so the
        // slider always reflects what will actually fit on the target.
        #[qproperty(i32, persistence_max_mib)]
        // Recognised distribution family, surfaced in the UI so the user
        // can see why a particular persistence scheme was selected.
        #[qproperty(QString, distro_label)]
        // Windows 11 installer customization (applied via autounattend.xml).
        // `windows_iso` / `linux_iso` reflect the detected OS of the source ISO.
        #[qproperty(bool, windows_iso)]
        #[qproperty(bool, linux_iso)]
        // Windows build number of the loaded ISO's install.wim (e.g. 26100), 0
        // if unknown / not a Windows ISO. Gates version-specific installer
        // options in QML (Windows 11 is build >= 22000).
        #[qproperty(i32, windows_build)]
        // Install-image arch ("x86"/"amd64"/"arm64"), empty if unknown. Lets the
        // unattend emit a single arch-matched component instead of all three.
        #[qproperty(QString, windows_arch)]
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
        // Disable Windows 11 24H2+ automatic BitLocker device-encryption
        // on first boot. Useful for dual-boot, lab, and IT-imaged systems.
        #[qproperty(bool, disable_bitlocker)]
        // Drop `SkuSiPolicy.p7b` onto the USB so older UEFI firmware can
        // boot the Windows-CA-2023-signed Microsoft bootloader chain.
        #[qproperty(bool, windows_ca_2023)]
        // Lay out a `USBooty\` folder of post-install .bat helpers
        // (Win11Debloat, ChrisTitus winutil, MAS, OneDrive removal,
        // OfficeTool) on the first user's Desktop. Specialize-pass
        // xcopies it into `C:\Users\Default\Desktop\USBooty\` so every
        // new account created in OOBE inherits the folder.
        #[qproperty(bool, desktop_helpers)]
        // Drop `sources/ei.cfg` so Setup ignores the firmware OEM key
        // (MSDM/SLIC) and presents its edition picker on boot. Useful
        // for installing Pro/Enterprise on a PC pre-baked Home Familiale.
        #[qproperty(bool, force_edition_picker)]
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
        // True once the activity log holds at least one line. The log body
        // itself is kept Rust-side (see `full_log` / `log_html`) and streamed
        // to the view via `append_log_html`, so appends stay O(1) instead of
        // rebuilding a whole QString each line. This flag just drives the
        // panel's auto-expand and the Save/Clear enabled states.
        #[qproperty(bool, log_non_empty)]
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
        // When true, the user has opted out of locale-based translation and
        // wants the GUI in its English source language. Toggled from the
        // ? menu's "Force English" item; live-switches via QTranslator.
        #[qproperty(bool, force_english)]
        // Advisory warning about missing external tools (empty if all present).
        #[qproperty(QString, dep_warning)]
        // Async-populated text for the device-Inspect dialog. Empty until
        // the user opens the dialog; set to a "Loading…" placeholder when
        // `request_inspect` fires, then overwritten with the worker output.
        #[qproperty(QString, inspect_text)]
        // Persistent flag: when true the activity-log column stays open
        // even with an empty buffer, instead of auto-expanding only when
        // the first log line arrives. Saved in `settings.json`.
        #[qproperty(bool, show_logs_always)]
        // Name every copied file in the activity log, not just the large ones
        // (lifts the per-file size threshold). Saved in `settings.json`.
        #[qproperty(bool, log_all_files)]
        // Newline-separated labels of the filesystems whose mkfs tools are
        // actually installed on the host; the QML combo binds to this so
        // a user only sees variants that will succeed.
        #[qproperty(QString, available_filesystems)]
        // Windows-download dialog: newline-separated language / option lists.
        #[qproperty(QString, win_languages)]
        #[qproperty(QString, win_options)]
        // QEMU boot-test capabilities, probed once at startup: whether
        // qemu-system-x86_64 is installed, whether /dev/kvm offers hardware
        // acceleration, and whether OVMF firmware is present for UEFI boot.
        #[qproperty(bool, qemu_available)]
        #[qproperty(bool, qemu_kvm)]
        #[qproperty(bool, qemu_uefi)]
        #[qproperty(bool, qemu_secureboot)]
        #[qproperty(bool, qemu_tpm)]
        // Host limits for the boot-test dialog's vCPU and memory sliders.
        #[qproperty(i32, qemu_cpus_max)]
        #[qproperty(i32, qemu_ram_max)]
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
        /// backup, the inverse of writing). The output is compressed when
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
        /// Trigger an off-thread `lsblk` + `udevadm info` + `smartctl` dump
        /// of the selected device. The result lands in
        /// [`inspect_text`](AppController::inspect_text); the QML "Inspect"
        /// dialog binds to that property and shows a "Loading…" placeholder
        /// while the workers run.
        #[qinvokable]
        fn request_inspect(self: Pin<&mut AppController>);
        /// Force the UI into English (true) or back to the system locale
        /// (false). Live-switches via QTranslator; no restart needed.
        #[qinvokable]
        fn apply_force_english(self: Pin<&mut AppController>, force: bool);
        /// Copy the host's locale (from `$LANG` / `$LC_ALL`) into `locale`
        /// and the host's IANA time-zone (from `/etc/timezone` or
        /// `/etc/localtime`) into the matching Microsoft TimeZone ID for
        /// the `timezone` field. Falls back to `en-US` / `UTC` when the
        /// host doesn't expose either.
        #[qinvokable]
        fn replicate_regional_from_host(self: Pin<&mut AppController>);
        /// Dump the current activity-log buffer to `path` (file:// URLs
        /// from QML's FileDialog are normalised). Reports success or the
        /// IO error via the status bar; never panics.
        #[qinvokable]
        fn save_log_to(self: Pin<&mut AppController>, path: &QString);
        /// Persist the "always show activity log" preference and update
        /// the Qt property so the QML layout reacts immediately. The
        /// log_non_empty auto-expand path keeps working when this is off.
        #[qinvokable]
        fn apply_show_logs_always(self: Pin<&mut AppController>, on: bool);
        /// Persist the "log every copied file" preference and update the Qt
        /// property. Read into each job's options so the helper lifts its
        /// per-file size threshold in the activity log.
        #[qinvokable]
        fn apply_log_all_files(self: Pin<&mut AppController>, on: bool);
        /// Apply parsed CLI startup arguments (preload ISO / select device).
        /// Called from QML once the engine has finished loading.
        #[qinvokable]
        fn apply_startup_args(self: Pin<&mut AppController>);
        /// Return the activity log as accumulated HTML. The QML view calls
        /// this to repopulate itself when its (lazily-loaded) panel is shown,
        /// since the live `append_log_html` stream only carries new lines.
        #[qinvokable]
        fn log_html_snapshot(self: &AppController) -> QString;
        /// Empty the activity log (both the saved plain text and the HTML
        /// buffer) and clear the non-empty flag; the view reacts by clearing.
        #[qinvokable]
        fn clear_log(self: Pin<&mut AppController>);
        /// Boot the selected device in QEMU to verify it boots, in BIOS/MBR
        /// mode (`uefi = false`) or UEFI mode (`uefi = true`), with `mem_mb`
        /// of RAM and optional KVM acceleration. With `snapshot = true` the
        /// device is opened in snapshot mode so the test never modifies it;
        /// `snapshot = false` persists writes (needed to run Windows OOBE to
        /// completion), mutating the real device.
        #[qinvokable]
        fn verify_boot(
            self: Pin<&mut AppController>,
            mem_mb: i32,
            cpus: i32,
            firmware: i32,
            q35: bool,
            audio: bool,
            kvm: bool,
            network: bool,
            snapshot: bool,
        );
        /// Live status of every dependency (required and optional) for the
        /// Dependencies dialog. One line per dependency, fields separated by
        /// the unit-separator byte (U+001F): present(0/1), group key, name,
        /// package, purpose.
        #[qinvokable]
        fn dependency_report(self: &AppController) -> QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Emitted when a job finishes (success or failure).
        #[qsignal]
        fn job_finished(self: Pin<&mut AppController>, success: bool, message: QString);
        /// Emitted once per new activity-log line, carrying its pre-formatted
        /// HTML. The QML view appends it incrementally.
        #[qsignal]
        fn append_log_html(self: Pin<&mut AppController>, html: QString);
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
    /// The helper's stdin; writing `cancel` here aborts it.
    pub stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
    /// Cancellation flag for the Windows-ISO download.
    pub download_abort: Option<Arc<AtomicBool>>,
}
