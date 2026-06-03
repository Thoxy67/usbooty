//! Copy `SkuSiPolicy.p7b` from `install.wim` to `EFI\Microsoft\Boot\` on
//! the USB so older UEFI firmwares can boot the Windows CA 2023–signed
//! Microsoft bootloader chain.
//!
//! Background: Microsoft rotated the Secure Boot CA chain in 2023 (the new
//! "Windows UEFI CA 2023" signs current Windows bootloaders). Firmwares
//! that don't pick up the new CA via Windows Update refuse the signed
//! `.efi` files. The `SkuSiPolicy.p7b` policy file *vouches for* the new
//! CA without needing a UEFI variable update; drop it at the canonical
//! `EFI\Microsoft\Boot\SkuSiPolicy.p7b` path and the firmware accepts the
//! chain on first boot.
//!
//! The file isn't in the ISO root; it lives inside `sources/install.wim`
//! at `Windows\System32\SecureBootUpdates\SkuSiPolicy.p7b`. Extracting it
//! needs `wimlib-imagex extract` against the *source* ISO (the destination
//! USB only has the WIM as a single file, not the unpacked tree). The
//! flag is best-effort: when wimlib isn't installed or the file isn't in
//! the WIM (older Windows builds), the step is logged and skipped, never
//! aborts the job.
//!
//! Reference: Microsoft KB5025885 and Rufus's `src/format.c` ApplyWinCa
//! routine.

use anyhow::{Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{emit, fsutil};

/// Path of the policy file inside `install.wim`. Windows itself reads from
/// `\Windows\System32\SecureBootUpdates\` so the layout matches.
const SOURCE_PATH: &str = "Windows/System32/SecureBootUpdates/SkuSiPolicy.p7b";

/// Extract `SkuSiPolicy.p7b` from `src_iso`'s install image and place it
/// at `<dest_mount>/EFI/Microsoft/Boot/SkuSiPolicy.p7b`. Returns `Ok(())`
/// on either a successful copy or a soft skip (with a log line).
pub fn apply(src_iso: &Path, dest_mount: &Path) -> Result<()> {
    if !fsutil::wimlib_available() {
        emit::log(
            "wimlib-imagex not installed; skipping Windows CA 2023 \
             policy install. Install `wimtools` / `wimlib` to enable it.",
        );
        return Ok(());
    }

    let iso_mount = fsutil::LoopMount::open_iso(src_iso, "winca-iso")?;
    let install_wim = fsutil::ci_path(iso_mount.path(), &["sources", "install.wim"])?;

    let staging = PathBuf::from(format!("/run/usbooty-winca-{}", std::process::id()));
    fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;

    // wimlib-imagex extract <wim> <image-index> <internal-path> --dest-dir <dir>
    // Image index 1 is "Windows Setup / install" in every Microsoft WIM.
    // `--nullglob`: a path that matches nothing is not an error, so a build
    // that doesn't ship SkuSiPolicy.p7b (or files it elsewhere) is a clean
    // skip rather than a hard `wimlib-imagex` failure that aborts the job.
    // Without it, recent builds (e.g. Win 11 25H2) abort with exit code 49.
    let mut child = Command::new("wimlib-imagex")
        .arg("extract")
        .arg(&install_wim)
        .arg("1")
        .arg(SOURCE_PATH)
        .arg("--dest-dir")
        .arg(&staging)
        .arg("--no-acls")
        .arg("--nullglob")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning wimlib-imagex extract")?;
    let status = child.wait().context("waiting for wimlib-imagex")?;
    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr);
        }
        let stderr = stderr.trim();
        // `SkuSiPolicy.p7b` is only present in some Windows builds, and at a
        // path that has shifted between releases. When the WIM simply doesn't
        // contain it, that's a soft-skip, not a failure. `--nullglob` above
        // already turns a no-match into a clean exit; this stays as a belt-
        // and-suspenders for wimlib versions that still report it, across the
        // several phrasings it uses ("Path not found", "No matches for path
        // pattern", "path does not exist in the WIM image").
        let s = stderr.to_ascii_lowercase();
        if s.contains("path not found")
            || s.contains("file not found")
            || s.contains("no matches for path")
            || s.contains("does not exist in the wim")
        {
            emit::log(format!(
                "{SOURCE_PATH} not found inside install.wim; skipping \
                 Windows CA 2023 policy (this Windows build doesn't ship it)."
            ));
            let _ = fs::remove_dir_all(&staging);
            return Ok(());
        }
        let _ = fs::remove_dir_all(&staging);
        anyhow::bail!("wimlib-imagex extract failed: {stderr}");
    }

    let staged_file = staging
        .join("Windows")
        .join("System32")
        .join("SecureBootUpdates")
        .join("SkuSiPolicy.p7b");
    if !staged_file.is_file() {
        // With --nullglob this is the normal "the file isn't in this WIM"
        // outcome: extract succeeds but matches nothing. Skip cleanly.
        emit::log(
            "SkuSiPolicy.p7b is not present in this install.wim; skipping \
             Windows CA 2023 policy (this Windows build doesn't ship it).",
        );
        let _ = fs::remove_dir_all(&staging);
        return Ok(());
    }

    // Resolve `EFI/Microsoft/Boot` case-insensitively against the directories
    // the ISO copied (lower-cased `efi/microsoft/boot` on real media). Joining
    // the hard-coded capitalised path on a case-sensitive destination (the
    // ntfs3 kernel driver) would create colliding sibling directories, which
    // corrupts the NTFS index from Windows' view. See [`fsutil::ci_join`].
    let dest = fsutil::ci_join(dest_mount, &["EFI", "Microsoft", "Boot", "SkuSiPolicy.p7b"]);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::copy(&staged_file, &dest)
        .with_context(|| format!("copying {} to {}", staged_file.display(), dest.display()))?;
    let _ = fs::remove_dir_all(&staging);

    emit::log(format!(
        "Installed Windows CA 2023 policy at {}",
        dest.display()
    ));
    Ok(())
}
