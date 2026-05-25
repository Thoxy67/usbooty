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

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::emit;

/// How often to poll `abort` while wimlib-imagex runs.
const POLL_EVERY: Duration = Duration::from_millis(250);

/// Chunk size in MiB. Rufus uses 4094 — sized just below the 4 GiB FAT32
/// single-file ceiling so split outputs always fit, with margin for the
/// `.swm` container overhead.
const CHUNK_MIB: u32 = 4094;

/// Mount `src_iso`, find `sources/install.wim`, and split it into <4 GiB
/// chunks under `<dest_mount>/sources/install.swm`. The destination must be
/// FAT32-formatted and writable; `install.wim` must NOT already exist there
/// (it would not have fit during the file-by-file copy, which is why this
/// path runs at all).
pub fn split_install_wim(src_iso: &Path, dest_mount: &Path, abort: &AtomicBool) -> Result<()> {
    if !wimlib_available() {
        bail!(
            "wimlib-imagex is required for the Split strategy — install the \
             `wimtools` / `wimlib` package and try again, or use UEFI:NTFS instead"
        );
    }

    let src_mount = mount_source_iso(src_iso)?;
    let src_wim = resolve_install_wim(src_mount.path())?;

    let dest_sources = dest_mount.join("sources");
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

/// True when the `wimlib-imagex` binary is on the helper's PATH.
pub fn wimlib_available() -> bool {
    Command::new("wimlib-imagex")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Loop-mount the source ISO read-only into a temporary directory, unmounted
/// on drop. Tries UDF first (modern Windows ISOs are UDF), then ISO9660.
struct SourceMount {
    mountpoint: PathBuf,
}

impl SourceMount {
    fn path(&self) -> &Path {
        &self.mountpoint
    }
}

impl Drop for SourceMount {
    fn drop(&mut self) {
        let _ = Command::new("umount").arg(&self.mountpoint).status();
        let _ = fs::remove_dir(&self.mountpoint);
    }
}

fn mount_source_iso(iso: &Path) -> Result<SourceMount> {
    let mountpoint = PathBuf::from(format!("/run/usbooty-wim-{}", std::process::id()));
    fs::create_dir_all(&mountpoint)
        .with_context(|| format!("creating mountpoint {}", mountpoint.display()))?;
    let mut last_err = String::new();
    for fstype in ["udf", "iso9660"] {
        let output = Command::new("mount")
            .args(["-t", fstype, "-o", "loop,ro"])
            .arg(iso)
            .arg(&mountpoint)
            .output()
            .context("running mount")?;
        if output.status.success() {
            return Ok(SourceMount { mountpoint });
        }
        last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    let _ = fs::remove_dir(&mountpoint);
    bail!("could not mount the source ISO for wim split: {last_err}");
}

/// Locate `sources/install.wim` under `root` case-insensitively (UDF
/// preserves case, ISO9660 may upper-case).
fn resolve_install_wim(root: &Path) -> Result<PathBuf> {
    let sources = ci_child(root, "sources").context("the ISO has no `sources` directory")?;
    ci_child(&sources, "install.wim").context("the ISO has no `sources/install.wim` to split")
}

fn ci_child(dir: &Path, name: &str) -> Result<PathBuf> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry.context("reading a directory entry")?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            return Ok(entry.path());
        }
    }
    bail!("{name} not found in {}", dir.display());
}
