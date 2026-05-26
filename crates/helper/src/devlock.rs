//! Per-device advisory locking, so two helper processes cannot race on the
//! same block device.
//!
//! The [`blockdev::open_exclusive`] `O_EXCL` open is the hard guarantee that
//! catches concurrent writers, but it triggers *after* polkit has already
//! prompted the user. Holding an `flock` on `/run/usbooty-<basename>.lock`
//! from the helper's first action means a second helper aborts immediately
//! with a clean message naming the conflict, before any of the destructive
//! steps run.
//!
//! Released automatically on drop — the kernel drops `flock`s when the last
//! reference to the open file is closed, so even an unclean helper exit
//! cannot leave a stale lock behind.

use anyhow::{Context, Result, bail};
use nix::fcntl::{Flock, FlockArg};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::emit;

/// RAII guard around an advisory exclusive lock on a per-device lockfile.
///
/// The lockfile itself is never written to; only its `flock` state matters.
/// Dropping this releases the lock (and on most Linux systems leaves the
/// empty lockfile behind in `/run`, which is fine — `/run` is tmpfs).
pub struct DeviceLock {
    /// Hold the underlying open file alive; the lock dies with this.
    _lock: Flock<std::fs::File>,
}

impl DeviceLock {
    /// Take an exclusive non-blocking lock on the lockfile derived from
    /// `device`. Returns an error containing the device basename when
    /// another usbooty job is already in flight against it.
    pub fn acquire(device: &Path) -> Result<Self> {
        let basename = device
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("device");
        let lock_path = format!("/run/usbooty-{basename}.lock");

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            // We never write to the file; only flock state matters. Truncate
            // would race against any concurrent holder, so leave existing
            // content alone.
            .truncate(false)
            .mode(0o644)
            .open(&lock_path)
            .with_context(|| format!("creating lockfile {lock_path}"))?;

        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => {
                emit::log(format!("Acquired device lock {lock_path}"));
                Ok(Self { _lock: lock })
            }
            Err((_file, _err)) => bail!(
                "another usbooty job is already running on {} — refusing to start a second one",
                device.display()
            ),
        }
    }
}
