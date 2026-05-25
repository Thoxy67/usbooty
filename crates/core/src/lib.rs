//! `usbooty-core` — the shared contract between the unprivileged GUI and the
//! privileged helper.
//!
//! This crate is deliberately tiny and dependency-light (only `serde`). It
//! contains the serializable message types exchanged across the privilege
//! boundary plus the *pure* decision logic that decides how a USB drive should
//! be laid out. Keeping that logic here means it can be unit-tested without any
//! hardware, Qt, network, or root access.

pub mod device;
pub mod iso_report;
pub mod job;
pub mod plan;
pub mod progress;
pub mod revocation;
pub mod udf;
pub mod uefi_ntfs;

pub use device::DeviceInfo;
pub use iso_report::{IsoReport, OsKind, PersistenceKind};
pub use job::{
    CheckMode, FileSystem, Job, JobOptions, PartitionTable, Persistence, WimStrategy, WindowsSetup,
};
pub use plan::{auto_filesystem, FAT32_MAX_FILE};
pub use progress::{LogLevel, ProgressMsg};
pub use uefi_ntfs::{validate_uefi_ntfs, UEFI_NTFS_IMG_SIZE};
