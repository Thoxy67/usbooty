//! Pure decision logic: given an [`IsoReport`], decide the filesystem and
//! large-`install.wim` strategy the partition method should use.
//!
//! Everything here is a pure function so it can be exhaustively unit-tested
//! with fabricated reports — no hardware, no root.

use crate::iso_report::{IsoReport, OsKind};
use crate::job::{FileSystem, WimStrategy};

/// The largest single file FAT32 can store: `0xFFFFFFFF` bytes (4 GiB − 1).
pub const FAT32_MAX_FILE: u64 = 0xFFFF_FFFF;

/// Decide the default filesystem and large-`install.wim` strategy for an ISO.
///
/// A Windows ISO whose `install.wim` is too large for a FAT32 single file gets
/// the UEFI:NTFS layout — an NTFS partition keeps the image intact, and a tiny
/// FAT partition carries a signed bootloader. Everything else uses a single
/// FAT32 partition. The GUI seeds its filesystem selector with this; the user
/// may still override it.
pub fn auto_filesystem(report: &IsoReport) -> (FileSystem, WimStrategy) {
    let oversized_wim = report.os_kind == OsKind::Windows
        && report
            .install_wim_size
            .is_some_and(|size| size > FAT32_MAX_FILE);
    if oversized_wim {
        (FileSystem::Ntfs, WimStrategy::UefiNtfs)
    } else {
        (FileSystem::Fat32, WimStrategy::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_iso(wim_size: u64) -> IsoReport {
        IsoReport {
            os_kind: OsKind::Windows,
            has_install_wim: true,
            install_wim_size: Some(wim_size),
            has_4gb_file: wim_size > FAT32_MAX_FILE,
            ..IsoReport::unknown(wim_size + 1_000_000)
        }
    }

    #[test]
    fn small_wim_windows_uses_fat32() {
        let (fs, wim) = auto_filesystem(&windows_iso(3_000_000_000));
        assert_eq!(fs, FileSystem::Fat32);
        assert_eq!(wim, WimStrategy::None);
    }

    #[test]
    fn large_wim_windows_uses_uefi_ntfs() {
        let (fs, wim) = auto_filesystem(&windows_iso(5_500_000_000));
        assert_eq!(fs, FileSystem::Ntfs);
        assert_eq!(wim, WimStrategy::UefiNtfs);
    }

    #[test]
    fn linux_iso_uses_fat32() {
        let mut report = IsoReport::unknown(2_000_000_000);
        report.os_kind = OsKind::Linux;
        report.has_isolinux = true;
        let (fs, wim) = auto_filesystem(&report);
        assert_eq!(fs, FileSystem::Fat32);
        assert_eq!(wim, WimStrategy::None);
    }
}
