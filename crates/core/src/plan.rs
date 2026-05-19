//! Pure decision logic: given an [`IsoReport`] and the user's choices, decide
//! the concrete partition layout the helper should produce.
//!
//! Everything here is a pure function so it can be exhaustively unit-tested
//! with fabricated reports — no hardware, no root.

use crate::iso_report::{IsoReport, OsKind};
use crate::job::{PartitionTable, WimStrategy};

/// The largest single file FAT32 can store: `0xFFFFFFFF` bytes (4 GiB − 1).
pub const FAT32_MAX_FILE: u64 = 0xFFFF_FFFF;

/// Whether the FAT32 method must ask the user how to handle this ISO.
///
/// A prompt is needed only for Windows ISOs whose `install.wim` is too large
/// to fit on FAT32 as a single file. Everything else has an unambiguous layout.
pub fn needs_wim_choice(report: &IsoReport) -> bool {
    report.os_kind == OsKind::Windows
        && report
            .install_wim_size
            .is_some_and(|size| size > FAT32_MAX_FILE)
}

/// The resolved layout the helper will produce for the FAT32 method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionScheme {
    /// Partition table type to write.
    pub table: PartitionTable,
    /// How to handle an oversized `install.wim`.
    pub wim_strategy: WimStrategy,
    /// Whether the job needs the downloaded `uefi-ntfs.img` resource.
    pub needs_uefi_ntfs_img: bool,
}

/// Resolve a [`PartitionScheme`] from an ISO report and the user's selections.
///
/// `user_wim` is the choice from the "split vs UEFI:NTFS" dialog; it is only
/// consulted when [`needs_wim_choice`] is true. When a choice is required but
/// none was supplied, this defaults to [`WimStrategy::Split`] (the most
/// firmware-compatible option).
pub fn choose_scheme(
    report: &IsoReport,
    table: PartitionTable,
    user_wim: Option<WimStrategy>,
) -> PartitionScheme {
    let wim_strategy = if needs_wim_choice(report) {
        match user_wim {
            Some(WimStrategy::UefiNtfs) => WimStrategy::UefiNtfs,
            // Split is the safe default; `None` is not valid here.
            _ => WimStrategy::Split,
        }
    } else {
        WimStrategy::None
    };

    PartitionScheme {
        table,
        wim_strategy,
        needs_uefi_ntfs_img: wim_strategy == WimStrategy::UefiNtfs,
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
    fn small_wim_needs_no_choice() {
        let report = windows_iso(3_000_000_000);
        assert!(!needs_wim_choice(&report));
        let scheme = choose_scheme(&report, PartitionTable::Gpt, None);
        assert_eq!(scheme.wim_strategy, WimStrategy::None);
        assert!(!scheme.needs_uefi_ntfs_img);
    }

    #[test]
    fn large_wim_needs_choice() {
        let report = windows_iso(5_500_000_000);
        assert!(needs_wim_choice(&report));
    }

    #[test]
    fn large_wim_defaults_to_split() {
        let report = windows_iso(5_500_000_000);
        let scheme = choose_scheme(&report, PartitionTable::Mbr, None);
        assert_eq!(scheme.wim_strategy, WimStrategy::Split);
        assert!(!scheme.needs_uefi_ntfs_img);
        assert_eq!(scheme.table, PartitionTable::Mbr);
    }

    #[test]
    fn large_wim_honors_uefi_ntfs_choice() {
        let report = windows_iso(5_500_000_000);
        let scheme = choose_scheme(&report, PartitionTable::Gpt, Some(WimStrategy::UefiNtfs));
        assert_eq!(scheme.wim_strategy, WimStrategy::UefiNtfs);
        assert!(scheme.needs_uefi_ntfs_img);
    }

    #[test]
    fn linux_iso_never_needs_choice() {
        let mut report = IsoReport::unknown(2_000_000_000);
        report.os_kind = OsKind::Linux;
        report.has_isolinux = true;
        assert!(!needs_wim_choice(&report));
        let scheme = choose_scheme(&report, PartitionTable::Gpt, None);
        assert_eq!(scheme.wim_strategy, WimStrategy::None);
    }
}
