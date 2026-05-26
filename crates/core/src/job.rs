//! The fully-resolved description of a write job.
//!
//! The GUI builds a [`Job`], serializes it to JSON, and feeds it to the
//! privileged helper on stdin. The helper executes it verbatim — it makes no
//! policy decisions of its own, so every choice is pinned down here.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::iso_report::{DistroFamily, PersistenceKind};

/// The partition table type to write (the user always chooses this explicitly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionTable {
    /// GUID Partition Table — for UEFI booting.
    Gpt,
    /// Master Boot Record — for legacy BIOS booting.
    Mbr,
    /// MBR with the data partition flagged bootable, intended for *both*
    /// legacy BIOS and UEFI (firmware loads `/EFI/BOOT/BOOT*.EFI` from the
    /// FAT-formatted partition via the UEFI fallback path). On-disk layout
    /// is identical to plain [`Mbr`]; the distinction is a UI promise that
    /// the user picked a dual-firmware boot.
    MbrBiosUefi,
    /// GPT with a synthesised *hybrid* MBR: slot 1 mirrors the GPT data
    /// partition as a real bootable entry so legacy BIOSes find it, slot 2
    /// is the protective `0xEE` entry covering the GPT areas. UEFI follows
    /// the GPT as normal. Apple-style. Some buggy firmwares dislike hybrid
    /// MBRs — see the warning in [`crate::plan`].
    HybridMbrGpt,
}

impl PartitionTable {
    /// Label shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            PartitionTable::Gpt => "GPT (UEFI)",
            PartitionTable::Mbr => "MBR (BIOS)",
            PartitionTable::MbrBiosUefi => "MBR (BIOS+UEFI)",
            PartitionTable::HybridMbrGpt => "Hybrid MBR+GPT (BIOS+UEFI)",
        }
    }
}

/// The filesystem to create on the target's main partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSystem {
    /// FAT32 — universal, but a 4 GiB per-file limit.
    Fat32,
    /// FAT16 — legacy, for tiny media (≤ 4 GiB).
    Fat16,
    /// NTFS — Windows-native, no practical file-size limit.
    Ntfs,
    /// exFAT — FAT successor, no 4 GiB limit, broad support.
    ExFat,
    /// ext4 — Linux-native, modern.
    Ext4,
    /// ext3 — Linux, older journaled.
    Ext3,
    /// ext2 — Linux, non-journaled.
    Ext2,
    /// UDF — cross-platform, no FAT size limits, needs udftools.
    Udf,
    /// Btrfs — copy-on-write, snapshots; needs btrfs-progs.
    Btrfs,
    /// XFS — high-throughput Linux filesystem; needs xfsprogs.
    Xfs,
    /// F2FS — flash-friendly Linux filesystem; needs f2fs-tools.
    F2fs,
    /// JFS — IBM journaled FS; needs jfsutils.
    Jfs,
    /// NILFS2 — log-structured with continuous snapshots; needs nilfs-utils.
    Nilfs2,
}

impl FileSystem {
    /// Short name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            FileSystem::Fat32 => "FAT32",
            FileSystem::Fat16 => "FAT16",
            FileSystem::Ntfs => "NTFS",
            FileSystem::ExFat => "exFAT",
            FileSystem::Ext4 => "ext4",
            FileSystem::Ext3 => "ext3",
            FileSystem::Ext2 => "ext2",
            FileSystem::Udf => "UDF",
            FileSystem::Btrfs => "Btrfs",
            FileSystem::Xfs => "XFS",
            FileSystem::F2fs => "F2FS",
            FileSystem::Jfs => "JFS",
            FileSystem::Nilfs2 => "NILFS2",
        }
    }

    /// The mkfs binary needed to create this filesystem. Used by the GUI
    /// to filter the combo box to filesystems actually supported by the
    /// installed userland, so a user never picks a variant whose tool is
    /// missing and gets a runtime failure mid-job.
    pub fn mkfs_tool(self) -> &'static str {
        match self {
            FileSystem::Fat32 | FileSystem::Fat16 => "mkfs.vfat",
            FileSystem::Ntfs => "mkfs.ntfs",
            FileSystem::ExFat => "mkfs.exfat",
            FileSystem::Ext4 => "mkfs.ext4",
            FileSystem::Ext3 => "mkfs.ext3",
            FileSystem::Ext2 => "mkfs.ext2",
            FileSystem::Udf => "mkudffs",
            FileSystem::Btrfs => "mkfs.btrfs",
            FileSystem::Xfs => "mkfs.xfs",
            FileSystem::F2fs => "mkfs.f2fs",
            FileSystem::Jfs => "mkfs.jfs",
            FileSystem::Nilfs2 => "mkfs.nilfs2",
        }
    }

    /// Every filesystem usbooty knows how to create, in UI display order.
    /// FAT32 first (the universal default), then size-class peers, then
    /// the Linux-native families ordered by adoption.
    pub fn all() -> &'static [FileSystem] {
        &[
            FileSystem::Fat32,
            FileSystem::Fat16,
            FileSystem::Ntfs,
            FileSystem::ExFat,
            FileSystem::Udf,
            FileSystem::Ext4,
            FileSystem::Ext3,
            FileSystem::Ext2,
            FileSystem::Btrfs,
            FileSystem::Xfs,
            FileSystem::F2fs,
            FileSystem::Jfs,
            FileSystem::Nilfs2,
        ]
    }
}

/// How a Windows `install.wim` larger than FAT32's 4 GiB file limit is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WimStrategy {
    /// No oversized image — a single partition holds everything as-is.
    None,
    /// The UEFI:NTFS two-partition layout: a large NTFS partition keeps
    /// `install.wim` intact, plus a tiny FAT partition with a signed bootloader.
    UefiNtfs,
    /// Split `install.wim` into <4 GiB chunks (`install.swm`, `install2.swm`,
    /// …) with `wimlib-imagex` and place them on a single FAT32 partition.
    /// Windows Setup loads `.swm` chunks natively. Broader firmware support
    /// than UEFI:NTFS, at the cost of needing `wimlib-imagex` on the host.
    Split,
}

/// Cross-cutting options that apply to every job mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOptions {
    /// Volume label for the main partition (sanitized per-filesystem by the
    /// helper). Empty falls back to a default.
    #[serde(default)]
    pub label: String,
    /// Zero the whole device before partitioning, rather than a quick format.
    #[serde(default)]
    pub full_format: bool,
    /// Read the written data back and verify it after the job completes.
    #[serde(default)]
    pub verify: bool,
}

/// An optional persistent overlay partition for a Linux live USB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Persistence {
    /// Which live-system persistence scheme to set up.
    pub kind: PersistenceKind,
    /// Size of the persistence partition, in bytes.
    pub size_bytes: u64,
}

/// Optional customization of a Windows installation, applied via a generated
/// `autounattend.xml` placed on the USB.
///
/// Every field is independent and emits its own block in the XML; an empty
/// [`WindowsSetup`] produces an unattend file with no `<settings>` elements,
/// which Windows ignores. The whole struct is designed for cross-version
/// compatibility — registry keys that exist only on Windows 11 (TPM bypass,
/// Copilot disable) are silently ignored on Windows 10, and the OOBE settings
/// used here are valid across all supported versions back to Windows 10 1809.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsSetup {
    /// Bypass the Windows 11 TPM 2.0 requirement.
    #[serde(default)]
    pub bypass_tpm: bool,
    /// Bypass the Windows 11 Secure Boot requirement.
    #[serde(default)]
    pub bypass_secureboot: bool,
    /// Bypass the Windows 11 8 GB RAM requirement.
    #[serde(default)]
    pub bypass_ram: bool,
    /// Bypass the Windows 11 64 GB system-disk minimum-size check.
    #[serde(default)]
    pub bypass_storage: bool,
    /// Bypass the Windows 11 supported-CPU allowlist check.
    #[serde(default)]
    pub bypass_cpu: bool,
    /// Bypass the Windows 11 disk geometry / partition style check.
    #[serde(default)]
    pub bypass_disk: bool,
    /// Skip the forced Microsoft-account requirement during OOBE. Emits both
    /// the `BypassNRO` registry write (Win 10 / Win 11 pre-24H2) and the
    /// `HideOnlineAccountScreens` OOBE element (Win 11 24H2+) so a single
    /// toggle works across the whole supported matrix.
    #[serde(default)]
    pub skip_msaccount: bool,
    /// Disable every network adapter during the `specialize` pass, then
    /// re-enable them in `FirstLogonCommands`. With no network during OOBE,
    /// Windows 11 24H2+ falls back to local-account creation even when
    /// `BypassNRO` and `HideOnlineAccountScreens` are silently ignored — the
    /// most robust local-account workaround currently known.
    #[serde(default)]
    pub disable_network_during_oobe: bool,
    /// Skip the forced Wi-Fi connection screen during OOBE — the Windows 11
    /// "Let's connect you to a network" page.
    #[serde(default)]
    pub hide_wireless_setup: bool,
    /// Hide the OEM-registration screen during OOBE.
    #[serde(default)]
    pub hide_oem_registration: bool,
    /// Pre-answer the OOBE "is this network private / a work network?" prompt
    /// with `Work` (a private trusted network).
    #[serde(default)]
    pub network_location_work: bool,
    /// Disable telemetry / data-collection prompts (hides the EULA page and
    /// sets `ProtectYourPC=3`, the "skip Express settings" answer).
    #[serde(default)]
    pub disable_telemetry: bool,
    /// Auto-accept the Setup-time EULA, so Setup proceeds without prompting.
    #[serde(default)]
    pub accept_eula: bool,
    /// Enable the legacy .NET Framework 3.5 component from the Windows
    /// installation media's `sources\sxs` folder — needed by many older apps
    /// and not installed by default since Windows 8.
    #[serde(default)]
    pub enable_dotnet35: bool,
    /// Create a local account with this name (skips account creation in OOBE).
    #[serde(default)]
    pub local_account: Option<String>,
    /// Password for [`local_account`](Self::local_account). When set, also
    /// emits an `<AutoLogon>` block so the first boot logs in directly.
    #[serde(default)]
    pub local_account_password: Option<String>,
    /// Set the machine name (a.k.a. hostname). 1-15 chars, no whitespace and
    /// no `\/:*?"<>|`; longer values are truncated by the helper.
    #[serde(default)]
    pub computer_name: Option<String>,
    /// Locale tag applied to setup UI, system locale, UI language, user
    /// locale, and the default keyboard input layout — e.g. `"en-US"`.
    #[serde(default)]
    pub locale: Option<String>,
    /// Microsoft time-zone identifier (e.g. `"UTC"`, `"Pacific Standard Time"`,
    /// `"Romance Standard Time"`). Free-form; Windows rejects unknown values.
    #[serde(default)]
    pub timezone: Option<String>,
    /// Product key to feed Setup. A generic VL key (e.g. the public Win 11 Pro
    /// `VK7JG-NPHTM-C97JM-9MPGT-3V66T`) lets Setup skip its activation prompt
    /// without actually activating the installation.
    #[serde(default)]
    pub product_key: Option<String>,
    /// Apply the vendored debloat policy: write `usbooty-debloat.reg` to the
    /// USB root and import it during the `specialize` pass (machine-wide
    /// policies via `HKLM`, default-user policies via `HKU\DFT`).
    #[serde(default)]
    pub apply_debloat: bool,
    /// Disable Windows 11 24H2+ automatic BitLocker device-encryption on
    /// first boot. Writes `HKLM\SYSTEM\CurrentControlSet\Control\BitLocker
    /// \PreventDeviceEncryption=1` during the `specialize` pass so OOBE
    /// never auto-encrypts the system drive. Useful for dual-boot setups,
    /// IT-imaged hardware, and labs that recover/clone disks regularly.
    /// Silently no-ops on older Windows versions that don't auto-encrypt.
    #[serde(default)]
    pub disable_bitlocker: bool,
    /// Copy `SkuSiPolicy.p7b` from `install.wim` to `EFI\Microsoft\Boot\`
    /// on the USB so older UEFI firmwares that haven't picked up the
    /// Windows CA 2023 chain through Windows Update can still boot the
    /// new-CA-signed Microsoft bootloader. Requires `wimlib-imagex` on
    /// the host; the helper falls back to a clear error if it's missing.
    #[serde(default)]
    pub windows_ca_2023: bool,
    /// Drop a `USBooty\` folder of post-install helper `.bat` scripts onto
    /// the first user's Desktop (Win11Debloat, ChrisTitus winutil, MAS,
    /// OneDrive removal, OfficeTool download). The folder is copied to
    /// `C:\Users\Default\Desktop\USBooty\` during the `specialize` pass,
    /// so Windows clones it into every new user account at OOBE.
    #[serde(default)]
    pub desktop_helpers: bool,
}

impl WindowsSetup {
    /// Whether any customization is actually requested.
    pub fn is_active(&self) -> bool {
        self.bypass_tpm
            || self.bypass_secureboot
            || self.bypass_ram
            || self.bypass_storage
            || self.bypass_cpu
            || self.bypass_disk
            || self.skip_msaccount
            || self.disable_network_during_oobe
            || self.hide_wireless_setup
            || self.hide_oem_registration
            || self.network_location_work
            || self.disable_telemetry
            || self.accept_eula
            || self.enable_dotnet35
            || self.local_account.is_some()
            || self.local_account_password.is_some()
            || self.computer_name.is_some()
            || self.locale.is_some()
            || self.timezone.is_some()
            || self.product_key.is_some()
            || self.apply_debloat
            || self.disable_bitlocker
            || self.windows_ca_2023
            || self.desktop_helpers
    }
}

/// A complete, executable description of one write operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Job {
    /// Raw byte-for-byte write of `iso_path` onto `device_path`.
    Dd {
        iso_path: PathBuf,
        device_path: PathBuf,
        #[serde(default)]
        opts: JobOptions,
    },
    /// Partition `device_path`, create a filesystem, and copy the ISO contents.
    Partitioned {
        iso_path: PathBuf,
        device_path: PathBuf,
        table: PartitionTable,
        /// Filesystem for the main partition.
        filesystem: FileSystem,
        /// Large-`install.wim` handling.
        wim: WimStrategy,
        /// Path to a locally-cached `uefi-ntfs.img`; required iff
        /// `wim == UefiNtfs`. The GUI downloads it so the helper never needs
        /// network access.
        #[serde(default)]
        uefi_ntfs_img: Option<PathBuf>,
        /// Optional persistent overlay partition for a Linux live USB.
        #[serde(default)]
        persistence: Option<Persistence>,
        /// Optional Windows-installer customization.
        #[serde(default)]
        windows_setup: Option<WindowsSetup>,
        /// When true, run `syslinux`/`extlinux` against the new partition and
        /// stamp a Syslinux MBR onto the device so the result boots on legacy
        /// BIOS. The GUI sets this for isolinux-based Linux ISOs.
        #[serde(default)]
        install_bootloader: bool,
        /// Distribution family detected from the ISO. Drives the post-copy
        /// quirk fix table; `Unknown` (default) skips every per-distro patch.
        #[serde(default)]
        distro: DistroFamily,
        #[serde(default)]
        opts: JobOptions,
    },
    /// Partition and format `device_path` with no payload — a blank, usable
    /// (non-bootable) drive.
    Format {
        device_path: PathBuf,
        table: PartitionTable,
        filesystem: FileSystem,
        #[serde(default)]
        opts: JobOptions,
    },
    /// Install or update Ventoy on `device_path` via the Ventoy CLI, then
    /// optionally copy `iso_path` onto the Ventoy data partition. Ventoy does
    /// its own partitioning and formatting, so this carries no `JobOptions`.
    Ventoy {
        device_path: PathBuf,
        /// GPT (Ventoy's `-g`) instead of Ventoy's MBR default.
        table: PartitionTable,
        /// Secure Boot support (Ventoy's `-s` / `-S`).
        secure_boot: bool,
        /// Update an existing Ventoy install (`-u`) instead of a fresh install.
        update: bool,
        /// An ISO to copy onto the Ventoy data partition once it is ready.
        #[serde(default)]
        iso_path: Option<PathBuf>,
    },
    /// Snapshot a device into an image file — the inverse of [`Job::Dd`].
    /// The output is compressed transparently when `image_path` ends in
    /// `.gz` / `.xz` / `.zst` / `.bz2`; otherwise the bytes are written raw.
    Backup {
        device_path: PathBuf,
        image_path: PathBuf,
        #[serde(default)]
        opts: JobOptions,
    },
    /// Run integrity checks on a device: a fast sample-based fake-capacity
    /// check, or a slow destructive bad-blocks scan.
    Check {
        device_path: PathBuf,
        mode: CheckMode,
    },
    /// Create a FreeDOS-bootable USB stick. No source ISO — the helper
    /// formats the device as FAT (16 or 32, user's choice), installs the
    /// FreeDOS boot sector, drops `KERNEL.SYS` + `COMMAND.COM` at the
    /// FAT root, and stamps a generic MBR. The GUI downloads the upstream
    /// FreeDOS files (one-shot, cached) and hands the helper local paths
    /// so the helper itself stays network-free.
    Freedos {
        device_path: PathBuf,
        table: PartitionTable,
        /// `Fat16` or `Fat32`; the helper rejects anything else.
        filesystem: FileSystem,
        /// Cached path to the FreeDOS `KERNEL.SYS`.
        kernel_sys: PathBuf,
        /// Cached path to the FreeDOS `COMMAND.COM`.
        command_com: PathBuf,
        /// Cached path to the matching FAT boot sector (`BOOT16.BIN` for
        /// FAT16, `BOOT32.BIN` for FAT32).
        boot_bin: PathBuf,
        #[serde(default)]
        opts: JobOptions,
    },
}

/// Intensity of a [`Job::Check`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckMode {
    /// F3-style sampling check — finishes in seconds; catches counterfeit drives.
    Quick,
    /// Two-pattern destructive bad-blocks scan over every sector.
    Full,
}

impl Job {
    /// The target device node for this job.
    pub fn device_path(&self) -> &PathBuf {
        match self {
            Job::Dd { device_path, .. }
            | Job::Partitioned { device_path, .. }
            | Job::Format { device_path, .. }
            | Job::Ventoy { device_path, .. }
            | Job::Backup { device_path, .. }
            | Job::Check { device_path, .. }
            | Job::Freedos { device_path, .. } => device_path,
        }
    }

    /// The source ISO for this job, if it has one.
    pub fn iso_path(&self) -> Option<&PathBuf> {
        match self {
            Job::Dd { iso_path, .. } | Job::Partitioned { iso_path, .. } => Some(iso_path),
            Job::Ventoy { iso_path, .. } => iso_path.as_ref(),
            Job::Format { .. } | Job::Backup { .. } | Job::Check { .. } | Job::Freedos { .. } => {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_roundtrips_through_json() {
        let job = Job::Partitioned {
            iso_path: "/tmp/win.iso".into(),
            device_path: "/dev/sdb".into(),
            table: PartitionTable::Gpt,
            filesystem: FileSystem::Ntfs,
            wim: WimStrategy::UefiNtfs,
            uefi_ntfs_img: Some("/home/u/.cache/usbooty/uefi-ntfs.img".into()),
            persistence: None,
            windows_setup: None,
            install_bootloader: false,
            distro: DistroFamily::Unknown,
            opts: JobOptions {
                label: "WIN11".into(),
                full_format: false,
                verify: true,
            },
        };
        let json = serde_json::to_string(&job).unwrap();
        let back: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(job, back);
    }

    #[test]
    fn dd_job_roundtrips() {
        let job = Job::Dd {
            iso_path: "/tmp/linux.iso".into(),
            device_path: "/dev/sdc".into(),
            opts: JobOptions::default(),
        };
        let back: Job = serde_json::from_str(&serde_json::to_string(&job).unwrap()).unwrap();
        assert_eq!(job, back);
    }
}
