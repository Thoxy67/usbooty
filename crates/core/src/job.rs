//! The fully-resolved description of a write job.
//!
//! The GUI builds a [`Job`], serializes it to JSON, and feeds it to the
//! privileged helper on stdin. The helper executes it verbatim — it makes no
//! policy decisions of its own, so every choice is pinned down here.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which of the two write methods the user selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteMethod {
    /// Raw byte-for-byte copy of the ISO onto the device.
    Dd,
    /// Partition the device and copy the ISO contents file-by-file.
    Fat32,
}

/// The partition table type to write (the user always chooses this explicitly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionTable {
    /// GUID Partition Table — for UEFI booting.
    Gpt,
    /// Master Boot Record — for legacy BIOS booting.
    Mbr,
}

impl PartitionTable {
    /// Label shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            PartitionTable::Gpt => "GPT (UEFI)",
            PartitionTable::Mbr => "MBR (BIOS/Legacy)",
        }
    }
}

/// How to handle a Windows `install.wim` that exceeds the FAT32 4 GiB file limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WimStrategy {
    /// No oversized file — copy everything as-is onto a single FAT32 partition.
    None,
    /// Split `install.wim` into `install.swm` chunks with `wimlib-imagex`.
    Split,
    /// Use the UEFI:NTFS two-partition trick (large NTFS + tiny FAT32 ESP).
    UefiNtfs,
}

/// A complete, executable description of one write operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Job {
    /// Raw write of `iso_path` onto `device_path`.
    Dd {
        iso_path: PathBuf,
        device_path: PathBuf,
    },
    /// Partition `device_path`, create filesystem(s), and copy the ISO contents.
    Partitioned {
        iso_path: PathBuf,
        device_path: PathBuf,
        table: PartitionTable,
        wim_strategy: WimStrategy,
        /// Path to a locally-cached `uefi-ntfs.img`; required iff
        /// `wim_strategy == UefiNtfs`. The GUI downloads it so the helper
        /// never needs network access.
        uefi_ntfs_img: Option<PathBuf>,
        /// Volume label detected from the source image, used to name the main
        /// partition and its filesystem. Empty when the image has no label —
        /// the helper then falls back to a default.
        label: String,
    },
}

impl Job {
    /// The target device node for this job.
    pub fn device_path(&self) -> &PathBuf {
        match self {
            Job::Dd { device_path, .. } | Job::Partitioned { device_path, .. } => device_path,
        }
    }

    /// The source ISO for this job.
    pub fn iso_path(&self) -> &PathBuf {
        match self {
            Job::Dd { iso_path, .. } | Job::Partitioned { iso_path, .. } => iso_path,
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
            wim_strategy: WimStrategy::UefiNtfs,
            uefi_ntfs_img: Some("/home/u/.cache/usbooty/uefi-ntfs.img".into()),
            label: "WIN11".into(),
        };
        let json = serde_json::to_string(&job).unwrap();
        let back: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(job, back);
    }
}
