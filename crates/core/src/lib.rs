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

pub use device::DeviceInfo;
pub use iso_report::{IsoReport, OsKind};
pub use job::{Job, PartitionTable, WimStrategy, WriteMethod};
pub use plan::{choose_scheme, needs_wim_choice, PartitionScheme, FAT32_MAX_FILE};
pub use progress::{LogLevel, ProgressMsg};
