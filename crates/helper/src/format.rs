//! The non-bootable "format only" method: partition and format a drive with
//! no payload, producing a blank usable disk.

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use usbooty_core::{FileSystem, JobOptions, PartitionTable};

use crate::{emit, partitioned};

/// Partition `device` with one `filesystem` partition and format it.
pub fn run(
    device: &Path,
    table: PartitionTable,
    filesystem: FileSystem,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    // `min_size` 0: there is no payload that must fit on the device.
    let _part = partitioned::setup_single_partition(device, table, filesystem, 0, opts, abort)?;
    emit::log(format!("{} drive formatted", filesystem.label()));
    Ok(())
}
