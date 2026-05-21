//! The result of analyzing an ISO image.
//!
//! Produced by the GUI's `iso` module (which reads the ISO9660 filesystem
//! directly), consumed by [`crate::plan`] to decide a partition layout.

use serde::{Deserialize, Serialize};

/// The live-USB persistence scheme an ISO supports, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceKind {
    /// Ubuntu / casper-based live systems — a `casper-rw` (or `writable`) partition.
    CasperRw,
    /// Debian-live — a `persistence` partition carrying a `persistence.conf` file.
    DebianLive,
    /// Fedora live — an ext4 partition whose label matches the ISO's volume
    /// label with an `-Live-overlay` suffix. dracut detects it at boot via the
    /// `rd.live.overlay` kernel parameter (which we add when patching configs).
    FedoraOverlay,
    /// openSUSE live (kiwi-live) — an ext4 partition labelled `cow`, picked up
    /// automatically by the live system; no kernel parameter required.
    OpenSuseCow,
}

/// The kind of operating system an ISO contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsKind {
    /// A Windows installation ISO (`sources/install.wim` + `bootmgr`/`setup.exe`).
    Windows,
    /// A Linux ISO (isolinux or GRUB present).
    Linux,
    /// A BSD ISO (FreeBSD / OpenBSD / NetBSD / …) — written with the DD method.
    Bsd,
    /// Something else — still writable with the DD method.
    Other,
}

/// A summary of everything the planner needs to know about an ISO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsoReport {
    /// Detected operating-system family.
    pub os_kind: OsKind,
    /// Whether the ISO carries an isohybrid MBR (directly DD-bootable).
    pub is_isohybrid: bool,
    /// `sources/install.wim` (or `.esd`/`.swm`) is present.
    pub has_install_wim: bool,
    /// Size of the install image in bytes, if present.
    pub install_wim_size: Option<u64>,
    /// The install image is `install.esd` (already-compressed) rather than `.wim`.
    pub install_wim_is_esd: bool,
    /// A `bootmgr` file is present in the root.
    pub has_bootmgr: bool,
    /// A `setup.exe` file is present in the root.
    pub has_setup_exe: bool,
    /// An `/EFI/BOOT/boot*.efi` UEFI bootloader directory is present.
    pub has_efi_boot_dir: bool,
    /// `isolinux.bin` / `isolinux.cfg` is present.
    pub has_isolinux: bool,
    /// A GRUB directory (`/boot/grub` or `/boot/grub2`) is present.
    pub has_grub: bool,
    /// Any single file is >= 4 GiB (cannot live on plain FAT32).
    pub has_4gb_file: bool,
    /// The ISO volume label.
    pub label: String,
    /// Total size of the ISO file on disk, in bytes.
    pub total_size: u64,
    /// Live-USB persistence support, if the ISO is a Linux live system.
    pub persistence: Option<PersistenceKind>,
    /// Warnings raised by scanning the ISO's signed EFI binaries against the
    /// current UEFI revocation database (SBAT levels, DBX hashes). Empty when
    /// no concern was found or the ISO carries no signed EFI binaries.
    #[serde(default)]
    pub revocation_warnings: Vec<String>,
}

impl IsoReport {
    /// A minimal report for an ISO we could not analyze (still DD-writable).
    pub fn unknown(total_size: u64) -> Self {
        IsoReport {
            os_kind: OsKind::Other,
            is_isohybrid: false,
            has_install_wim: false,
            install_wim_size: None,
            install_wim_is_esd: false,
            has_bootmgr: false,
            has_setup_exe: false,
            has_efi_boot_dir: false,
            has_isolinux: false,
            has_grub: false,
            has_4gb_file: false,
            label: String::new(),
            total_size,
            persistence: None,
            revocation_warnings: Vec::new(),
        }
    }

    /// A short human-readable summary line for the UI.
    pub fn summary(&self) -> String {
        let os = match self.os_kind {
            OsKind::Windows => "Windows",
            OsKind::Linux => "Linux",
            OsKind::Bsd => "BSD",
            OsKind::Other => "Generic image",
        };
        format!("{os} · {}", crate::device::format_size(self.total_size))
    }
}
