//! Detection of the external command-line tools usbooty relies on.
//!
//! Detection is best-effort and advisory: a missing tool only matters once a
//! method that needs it is used, and the privileged helper re-checks and
//! reports a clear error of its own. This module just lets the UI warn early.

use usbooty_core::FileSystem;

/// Which area a dependency belongs to. Drives both the grouping in the
/// Dependencies dialog and whether a missing tool is worth a startup banner.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    /// The app cannot run without it.
    Required,
    /// A filesystem formatter (`mkfs.*` + `mkudffs`).
    Filesystem,
    /// A backend behind a specific write method (Ventoy, FreeDOS, …).
    Feature,
    /// Used only by the QEMU boot test.
    BootTest,
    /// Pure quality-of-life; the app degrades silently when absent.
    QualityOfLife,
}

impl DepKind {
    /// Missing tools of these kinds are surfaced in the startup banner; the
    /// boot-test and quality-of-life kinds degrade in-context instead.
    fn in_banner(self) -> bool {
        matches!(
            self,
            DepKind::Required | DepKind::Filesystem | DepKind::Feature
        )
    }

    /// Stable key the QML Dependencies dialog groups rows by.
    pub fn key(self) -> &'static str {
        match self {
            DepKind::Required => "required",
            DepKind::Filesystem => "filesystem",
            DepKind::Feature => "feature",
            DepKind::BootTest => "boot",
            DepKind::QualityOfLife => "qol",
        }
    }
}

/// How a dependency's presence is tested.
enum Probe {
    /// An executable on `PATH` (or a standard `sbin` dir).
    Bin,
    /// A specific file/node exists (e.g. `/dev/kvm`).
    Path(&'static str),
    /// OVMF UEFI firmware is installed (see [`crate::qemu::uefi_available`]).
    Ovmf,
}

/// One dependency the app or helper may use, and how to detect it.
struct DepSpec {
    kind: DepKind,
    /// User-facing identifier: the binary name, or a component name for the
    /// non-binary entries (OVMF firmware, the KVM device).
    name: &'static str,
    /// Common package name across the major distro families.
    package: &'static str,
    /// One-line note on what it's for.
    purpose: &'static str,
    probe: Probe,
}

/// Every dependency the app or helper may use, required and optional. This is
/// the single source of truth for both the startup banner and the
/// Dependencies dialog. Keep it in sync with the `optdepends` array in
/// `packaging/PKGBUILD` and the AppImage host-requirements section in
/// `packaging/appimage/README.md`.
const DEPS: &[DepSpec] = &[
    DepSpec {
        kind: DepKind::Required,
        name: "pkexec",
        package: "polkit",
        purpose: "Privilege escalation for every device write",
        probe: Probe::Bin,
    },
    // Filesystem formatters — one per FS the user can pick. mkfs.ext2/ext3
    // ship with e2fsprogs alongside mkfs.ext4, so ext4 stands in for all three.
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.vfat",
        package: "dosfstools",
        purpose: "FAT16 / FAT32 formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.ntfs",
        package: "ntfs-3g",
        purpose: "NTFS formatting (Windows media, UEFI:NTFS)",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.exfat",
        package: "exfatprogs",
        purpose: "exFAT formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.ext4",
        package: "e2fsprogs",
        purpose: "ext2 / ext3 / ext4 formatting + Linux persistence",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkudffs",
        package: "udftools",
        purpose: "UDF formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.btrfs",
        package: "btrfs-progs",
        purpose: "Btrfs formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.xfs",
        package: "xfsprogs",
        purpose: "XFS formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.f2fs",
        package: "f2fs-tools",
        purpose: "F2FS formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.jfs",
        package: "jfsutils",
        purpose: "JFS formatting",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Filesystem,
        name: "mkfs.nilfs2",
        package: "nilfs-utils",
        purpose: "NILFS2 formatting",
        probe: Probe::Bin,
    },
    // Feature backends — each sits behind a specific write method.
    DepSpec {
        kind: DepKind::Feature,
        name: "ventoy",
        package: "ventoy",
        purpose: "Ventoy multi-boot USB creation",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Feature,
        name: "wimlib-imagex",
        package: "wimlib",
        purpose: "Split install.wim for Windows installers on FAT32",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Feature,
        name: "syslinux",
        package: "syslinux",
        purpose: "BIOS / legacy MBR bootloader for Linux ISOs",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Feature,
        name: "xorriso",
        package: "libisoburn",
        purpose: "Advanced ISO inspection / repair",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::Feature,
        name: "mformat",
        package: "mtools",
        purpose: "FreeDOS bootable USB (mformat / mcopy)",
        probe: Probe::Bin,
    },
    // Boot test (Device → Verify boot device).
    DepSpec {
        kind: DepKind::BootTest,
        name: "qemu-system-x86_64",
        package: "qemu-full",
        purpose: "Boot-test a device in a virtual machine",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::BootTest,
        name: "OVMF firmware",
        package: "edk2-ovmf",
        purpose: "UEFI firmware for the boot test (BIOS works without it)",
        probe: Probe::Ovmf,
    },
    DepSpec {
        kind: DepKind::BootTest,
        name: "KVM (/dev/kvm)",
        package: "kernel virtualization",
        purpose: "Hardware acceleration for the boot test",
        probe: Probe::Path("/dev/kvm"),
    },
    // Quality-of-life integrations — attempted in-context, silent on absence.
    DepSpec {
        kind: DepKind::QualityOfLife,
        name: "smartctl",
        package: "smartmontools",
        purpose: "SMART health probe + Inspect-dialog SMART panel",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::QualityOfLife,
        name: "udisksctl",
        package: "udisks2",
        purpose: "Auto-mount + open the Ventoy data partition",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::QualityOfLife,
        name: "xdg-open",
        package: "xdg-utils",
        purpose: "Open the data partition in your file manager",
        probe: Probe::Bin,
    },
    DepSpec {
        kind: DepKind::QualityOfLife,
        name: "notify-send",
        package: "libnotify",
        purpose: "Desktop notification when a long job finishes",
        probe: Probe::Bin,
    },
];

/// Whether a dependency is present right now.
fn present(spec: &DepSpec) -> bool {
    match spec.probe {
        Probe::Bin => on_path(spec.name),
        Probe::Path(p) => std::path::Path::new(p).exists(),
        Probe::Ovmf => crate::qemu::uefi_available(),
    }
}

/// One dependency's live status, for the Dependencies dialog.
pub struct DepStatus {
    /// Grouping key (see [`DepKind::key`]).
    pub kind_key: &'static str,
    pub name: &'static str,
    pub package: &'static str,
    pub purpose: &'static str,
    pub present: bool,
}

/// Every dependency with its current presence, for the Dependencies dialog.
pub fn full_report() -> Vec<DepStatus> {
    DEPS.iter()
        .map(|d| DepStatus {
            kind_key: d.kind.key(),
            name: d.name,
            package: d.package,
            purpose: d.purpose,
            present: present(d),
        })
        .collect()
}

/// Build a one-line warning about missing tools, or an empty string when
/// everything needed is present. Only banner-worthy kinds (required tools,
/// filesystem formatters, feature backends) are flagged; the boot-test and
/// quality-of-life tools degrade in-context instead.
pub fn warning() -> String {
    let missing: Vec<&DepSpec> = DEPS
        .iter()
        .filter(|d| d.kind.in_banner() && !present(d))
        .collect();
    if missing.is_empty() {
        return String::new();
    }

    let list = missing
        .iter()
        .map(|d| format!("{} ({})", d.name, d.package))
        .collect::<Vec<_>>()
        .join(", ");

    if missing.iter().any(|d| d.kind == DepKind::Required) {
        format!("Required tools are missing. Install: {list}")
    } else {
        format!("Optional tools are missing (some methods unavailable). Install: {list}")
    }
}

/// Return the filesystems usbooty can actually create on this host — i.e.
/// the ones whose `mkfs` tool is installed. The GUI binds its filesystem
/// combo to this list so a user is never offered something that would fail
/// at format time. FAT32 always appears even if `mkfs.vfat` is missing, so
/// the UI never collapses to an empty combo; the helper will surface a
/// clear error if the tool is truly absent.
pub fn available_filesystems() -> Vec<FileSystem> {
    let mut out: Vec<FileSystem> = FileSystem::all()
        .iter()
        .copied()
        .filter(|fs| on_path(fs.mkfs_tool()))
        .collect();
    if out.is_empty() {
        out.push(FileSystem::Fat32);
    }
    out
}

/// Whether `bin` is an executable file on `PATH` or in a standard `sbin`
/// directory (filesystem tools often live in `/usr/sbin`, which a
/// desktop-launched process may not have on `PATH`).
pub(crate) fn on_path(bin: &str) -> bool {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    for extra in ["/usr/sbin", "/sbin", "/usr/local/sbin"] {
        dirs.push(extra.into());
    }
    dirs.iter().any(|dir| dir.join(bin).is_file())
}
