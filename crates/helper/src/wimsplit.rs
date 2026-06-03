//! Split a Windows `install.wim` into <4 GiB chunks (`install.swm`) via
//! `wimlib-imagex split`, so a Windows installer ISO can live on a plain
//! FAT32 USB drive without the UEFI:NTFS two-partition workaround.
//!
//! The work is done *during* the partitioned copy: `install.wim` is excluded
//! from the file-by-file copy (it would not fit on FAT32) and this module
//! then mounts the source ISO read-only, runs wimlib against the original
//! image, and writes the resulting `install.swm` / `install2.swm` / ... chunks
//! directly onto the destination FAT32 partition. Windows Setup loads SWM
//! chunks natively, so no boot-config tweak is needed.

use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::{emit, fsutil};

/// How often to poll `abort` while wimlib-imagex runs.
const POLL_EVERY: Duration = Duration::from_millis(250);

/// Chunk size in MiB. Rufus uses 4094, sized just below the 4 GiB FAT32
/// single-file ceiling so split outputs always fit, with margin for the
/// `.swm` container overhead.
const CHUNK_MIB: u32 = 4094;

/// Mount `src_iso`, find `sources/install.wim`, and split it into <4 GiB
/// chunks under `<dest_mount>/sources/install.swm`. The destination must be
/// FAT32-formatted and writable; `install.wim` must NOT already exist there
/// (it would not have fit during the file-by-file copy, which is why this
/// path runs at all).
pub fn split_install_wim(src_iso: &Path, dest_mount: &Path, abort: &AtomicBool) -> Result<()> {
    if !fsutil::wimlib_available() {
        bail!(
            "wimlib-imagex is required for the Split strategy; install the \
             `wimtools` / `wimlib` package and try again, or use UEFI:NTFS instead"
        );
    }

    let src_mount = fsutil::LoopMount::open_iso(src_iso, "wim")?;
    let src_wim = fsutil::ci_path(src_mount.path(), &["sources", "install.wim"])
        .context("the ISO has no `sources/install.wim` to split")?;

    // Resolve the copied `sources/` case-insensitively so the chunks land in
    // the directory the ISO created rather than a hard-coded sibling that a
    // case-sensitive destination would treat as distinct. See
    // [`fsutil::ci_join`].
    let dest_sources = fsutil::ci_join(dest_mount, &["sources"]);
    fs::create_dir_all(&dest_sources)
        .with_context(|| format!("creating {}", dest_sources.display()))?;
    let swm_template = dest_sources.join("install.swm");

    emit::phase("Splitting install.wim");
    emit::log(format!(
        "Splitting {} into {CHUNK_MIB} MiB chunks at {}",
        src_wim.display(),
        swm_template.display()
    ));

    // Spawn rather than .output() so the cancel button can interrupt a split
    // that on a 5 GiB install.wim runs for ten-plus minutes.
    let mut child = Command::new("wimlib-imagex")
        .arg("split")
        .arg(&src_wim)
        .arg(&swm_template)
        .arg(CHUNK_MIB.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning wimlib-imagex")?;

    let status = loop {
        if abort.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("aborted by user");
        }
        match child.try_wait().context("waiting for wimlib-imagex")? {
            Some(status) => break status,
            None => thread::sleep(POLL_EVERY),
        }
    };

    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr);
        }
        bail!("wimlib-imagex split failed: {}", stderr.trim());
    }
    emit::log("install.wim split into install.swm chunks");
    Ok(())
}
