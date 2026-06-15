//! Backing storage for the `AppController` QObject and its initial state.

use cxx_qt_lib::QString;
use usbooty_core::{DeviceInfo, FileSystem, IsoReport};

use super::JobHandle;

/// Backing storage for [`qobject::AppController`].
pub struct AppControllerRust {
    pub(crate) iso_path: QString,
    pub(crate) iso_summary: QString,
    pub(crate) label: QString,
    pub(crate) iso_sha256: QString,
    pub(crate) iso_md5: QString,
    pub(crate) iso_sha1: QString,
    pub(crate) iso_adguard_badge: QString,
    pub(crate) iso_sha512: QString,
    pub(crate) iso_blake3: QString,
    pub(crate) hash_progress: f64,
    pub(crate) hashing: bool,
    pub(crate) app_version: QString,
    pub(crate) devices: QString,
    pub(crate) selected_device: i32,
    pub(crate) show_fixed_disks: bool,
    pub(crate) method: i32,
    pub(crate) table: i32,
    pub(crate) filesystem: i32,
    pub(crate) ventoy_update: bool,
    pub(crate) ventoy_secure_boot: bool,
    pub(crate) full_format: bool,
    pub(crate) verify: bool,
    pub(crate) persistence_supported: bool,
    pub(crate) persistence_note_key: QString,
    pub(crate) persistence_inline: bool,
    pub(crate) persistence_size: i32,
    pub(crate) persistence_max_mib: i32,
    pub(crate) distro_label: QString,
    pub(crate) windows_iso: bool,
    pub(crate) linux_iso: bool,
    pub(crate) windows_build: i32,
    pub(crate) windows_arch: QString,
    pub(crate) split_wim: bool,
    pub(crate) bypass_tpm: bool,
    pub(crate) bypass_secureboot: bool,
    pub(crate) bypass_ram: bool,
    pub(crate) bypass_storage: bool,
    pub(crate) bypass_cpu: bool,
    pub(crate) bypass_disk: bool,
    pub(crate) skip_msaccount: bool,
    pub(crate) disable_network_during_oobe: bool,
    pub(crate) hide_wireless_setup: bool,
    pub(crate) hide_oem_registration: bool,
    pub(crate) network_location_work: bool,
    pub(crate) disable_telemetry: bool,
    pub(crate) accept_eula: bool,
    pub(crate) enable_dotnet35: bool,
    pub(crate) apply_debloat: bool,
    pub(crate) disable_bitlocker: bool,
    pub(crate) windows_ca_2023: bool,
    pub(crate) desktop_helpers: bool,
    pub(crate) force_edition_picker: bool,
    pub(crate) show_file_extensions: bool,
    pub(crate) show_hidden_files: bool,
    pub(crate) classic_context_menu: bool,
    pub(crate) dark_mode: bool,
    pub(crate) disable_fast_startup: bool,
    pub(crate) local_account: QString,
    pub(crate) local_account_password: QString,
    pub(crate) prevent_password_expiration: bool,
    pub(crate) computer_name: QString,
    pub(crate) locale: QString,
    pub(crate) timezone: QString,
    pub(crate) product_key: QString,
    pub(crate) timezone_labels: QString,
    pub(crate) timezone_ids: QString,
    pub(crate) busy: bool,
    pub(crate) progress: f64,
    pub(crate) phase: QString,
    pub(crate) log_non_empty: bool,
    pub(crate) status: QString,
    pub(crate) speed: QString,
    pub(crate) eta: QString,
    pub(crate) fit_warning: QString,
    pub(crate) revocation_warnings: QString,
    pub(crate) smart_warning: QString,
    pub(crate) force_english: bool,
    pub(crate) show_logs_always: bool,
    pub(crate) log_all_files: bool,
    pub(crate) dep_warning: QString,
    pub(crate) inspect_text: QString,
    pub(crate) available_filesystems: QString,
    /// Parallel to `available_filesystems`: which FileSystem each combo
    /// index actually resolves to. Built once from `deps` detection at
    /// startup; never changes during a session.
    pub(crate) available_filesystem_kinds: Vec<FileSystem>,
    pub(crate) win_releases: QString,
    pub(crate) win_languages: QString,
    pub(crate) win_language_default: i32,
    pub(crate) win_options: QString,
    pub(crate) uefi_shells: QString,
    pub(crate) qemu_available: bool,
    pub(crate) qemu_kvm: bool,
    pub(crate) qemu_uefi: bool,
    pub(crate) qemu_secureboot: bool,
    pub(crate) qemu_tpm: bool,
    pub(crate) qemu_cpus_max: i32,
    pub(crate) qemu_ram_max: i32,
    /// Enumerated devices, parallel to the `devices` display strings.
    pub(crate) device_list: Vec<DeviceInfo>,
    /// Analysis of the currently selected ISO.
    pub(crate) iso_report: Option<IsoReport>,
    /// Languages fetched for the Windows-download dialog.
    pub win_catalog: Option<crate::windisco::Catalog>,
    /// Download options fetched for the selected language.
    pub win_option_list: Vec<crate::windisco::DownloadOption>,
    /// The flattened UEFI Shell download list, parallel to `uefi_shells`.
    pub uefi_shell_list: Vec<crate::windisco::ShellOption>,
    /// Present while a job runs; cleared by the runner when it finishes.
    pub job: Option<JobHandle>,
    /// Bumped whenever the loaded ISO changes (or hashing restarts) so an
    /// in-flight hash worker bound to a previous ISO discards its results
    /// instead of publishing them under the wrong image.
    pub(crate) hash_generation: u64,
    /// Plain-text activity log: the source of truth for "Save log".
    pub full_log: String,
    /// The same log accumulated as HTML, handed to the QML view via
    /// `log_html_tail` when its lazily-loaded panel (re)appears.
    pub log_html: String,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        // Compute the values that would otherwise be queried twice. Both
        // settings::load (disk read + JSON parse) and
        // deps::available_filesystems (walks PATH for one mkfs.* per
        // filesystem) are hot paths during the constructor.
        let prefs = crate::settings::Settings::load();
        let fs_kinds = crate::deps::available_filesystems();
        let fs_labels = fs_kinds
            .iter()
            .map(|fs| fs.label())
            .collect::<Vec<_>>()
            .join("\n");
        let qemu_caps = crate::qemu::detect();
        let uefi_shell_list = crate::windisco::uefi_shell_options();
        let uefi_shells = uefi_shell_list
            .iter()
            .map(|o| o.label.clone())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            iso_path: QString::default(),
            iso_summary: QString::from("No image selected"),
            label: QString::default(),
            iso_sha256: QString::default(),
            iso_md5: QString::default(),
            iso_sha1: QString::default(),
            iso_adguard_badge: QString::default(),
            iso_sha512: QString::default(),
            iso_blake3: QString::default(),
            hash_progress: 0.0,
            hashing: false,
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
            persistence_note_key: QString::default(),
            persistence_inline: false,
            persistence_size: 0,
            persistence_max_mib: 0,
            distro_label: QString::default(),
            windows_iso: false,
            linux_iso: false,
            windows_build: 0,
            windows_arch: QString::default(),
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
            disable_bitlocker: false,
            windows_ca_2023: false,
            desktop_helpers: false,
            force_edition_picker: false,
            show_file_extensions: false,
            show_hidden_files: false,
            classic_context_menu: false,
            dark_mode: false,
            disable_fast_startup: false,
            local_account: QString::default(),
            local_account_password: QString::default(),
            prevent_password_expiration: false,
            computer_name: QString::default(),
            locale: QString::default(),
            timezone: QString::default(),
            product_key: QString::default(),
            timezone_labels: QString::from(&crate::timezones::labels()),
            timezone_ids: QString::from(&crate::timezones::ids()),
            busy: false,
            progress: 0.0,
            phase: QString::default(),
            log_non_empty: false,
            status: QString::from("Ready"),
            speed: QString::default(),
            eta: QString::default(),
            fit_warning: QString::default(),
            revocation_warnings: QString::default(),
            smart_warning: QString::default(),
            force_english: prefs.force_english,
            show_logs_always: prefs.show_logs_always,
            log_all_files: prefs.log_all_files,
            dep_warning: QString::from(&crate::deps::warning()),
            inspect_text: QString::default(),
            available_filesystems: QString::from(&fs_labels),
            available_filesystem_kinds: fs_kinds,
            win_releases: QString::from(
                &crate::windisco::RELEASES
                    .iter()
                    .map(|r| r.name)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            win_languages: QString::default(),
            win_language_default: 0,
            win_options: QString::default(),
            uefi_shells: QString::from(&uefi_shells),
            qemu_available: qemu_caps.qemu,
            qemu_kvm: qemu_caps.kvm,
            qemu_uefi: qemu_caps.uefi,
            qemu_secureboot: crate::qemu::secureboot_available(),
            qemu_tpm: qemu_caps.tpm,
            qemu_cpus_max: crate::qemu::host_cpus() as i32,
            qemu_ram_max: crate::qemu::host_ram_mb() as i32,
            device_list: Vec::new(),
            iso_report: None,
            win_catalog: None,
            win_option_list: Vec::new(),
            uefi_shell_list,
            job: None,
            hash_generation: 0,
            full_log: String::new(),
            log_html: String::new(),
        }
    }
}
