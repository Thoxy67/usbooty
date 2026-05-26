//! Detection of the external command-line tools usbooty relies on.
//!
//! Detection is best-effort and advisory: a missing tool only matters once a
//! method that needs it is used, and the privileged helper re-checks and
//! reports a clear error of its own. This module just lets the UI warn early.

use usbooty_core::FileSystem;

/// An external tool, and the package that typically provides it.
struct Tool {
    /// Executable name.
    bin: &'static str,
    /// Common package name (across the major distro families).
    package: &'static str,
    /// Whether the app is unusable without it.
    critical: bool,
}

/// Every external tool the app or helper may invoke. Keep this list in sync
/// with the `optdepends` array in `packaging/PKGBUILD` (and the AppImage
/// host-requirements section in `packaging/appimage/README.md`).
const TOOLS: &[Tool] = &[
    // Mandatory — there's no fallback if pkexec is missing.
    Tool {
        bin: "pkexec",
        package: "polkit",
        critical: true,
    },
    // Filesystem formatters — one per FS the user can pick.
    Tool {
        bin: "mkfs.vfat",
        package: "dosfstools",
        critical: false,
    },
    Tool {
        bin: "mkfs.ntfs",
        package: "ntfs-3g",
        critical: false,
    },
    Tool {
        bin: "mkfs.exfat",
        package: "exfatprogs",
        critical: false,
    },
    Tool {
        bin: "mkfs.ext4",
        package: "e2fsprogs",
        critical: false,
    },
    // mkfs.ext2 / mkfs.ext3 ship with e2fsprogs alongside mkfs.ext4, so we
    // don't probe them separately — the e2fsprogs presence is checked once.
    Tool {
        bin: "mkudffs",
        package: "udftools",
        critical: false,
    },
    // The following are picked up dynamically by `available_filesystems()`
    // for the combo box; we don't warn the user about their absence because
    // they're niche choices most installs won't have. A user who selects
    // one without the tool sees a clear helper error at format time.
    Tool { bin: "mkfs.btrfs",  package: "btrfs-progs", critical: false },
    Tool { bin: "mkfs.xfs",    package: "xfsprogs",    critical: false },
    Tool { bin: "mkfs.f2fs",   package: "f2fs-tools",  critical: false },
    Tool { bin: "mkfs.jfs",    package: "jfsutils",    critical: false },
    Tool { bin: "mkfs.nilfs2", package: "nilfs-utils", critical: false },
    // Optional feature backends.
    Tool {
        bin: "ventoy",
        package: "ventoy",
        critical: false,
    },
    Tool {
        bin: "wimlib-imagex",
        package: "wimlib",
        critical: false,
    },
    Tool {
        bin: "syslinux",
        package: "syslinux",
        critical: false,
    },
    Tool {
        bin: "xorriso",
        package: "libisoburn",
        critical: false,
    },
    // Quality-of-life integrations — silently degrade if absent, so they are
    // not flagged in the dep banner; included here for documentation only.
    // (Uncomment the entries if a future change makes them load-bearing.)
    // Tool { bin: "smartctl",  package: "smartmontools", critical: false },
    // Tool { bin: "udisksctl", package: "udisks2",       critical: false },
    // Tool { bin: "xdg-open",  package: "xdg-utils",     critical: false },
    // Tool { bin: "notify-send", package: "libnotify",   critical: false },
];

/// Build a one-line warning about missing tools, or an empty string when
/// everything needed is present.
pub fn warning() -> String {
    let missing: Vec<&Tool> = TOOLS.iter().filter(|t| !on_path(t.bin)).collect();
    if missing.is_empty() {
        return String::new();
    }

    let list = missing
        .iter()
        .map(|t| format!("{} ({})", t.bin, t.package))
        .collect::<Vec<_>>()
        .join(", ");

    if missing.iter().any(|t| t.critical) {
        format!("Required tools are missing — install: {list}")
    } else {
        format!("Optional tools are missing (some methods unavailable) — install: {list}")
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
fn on_path(bin: &str) -> bool {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    for extra in ["/usr/sbin", "/sbin", "/usr/local/sbin"] {
        dirs.push(extra.into());
    }
    dirs.iter().any(|dir| dir.join(bin).is_file())
}
