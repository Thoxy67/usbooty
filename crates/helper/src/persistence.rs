//! Setting up a Linux live-USB persistence partition (the writable overlay
//! that lets changes survive a reboot).

use anyhow::{Context, Result};
use std::path::Path;

use usbooty_core::PersistenceKind;

use crate::{emit, fsutil};

/// Format and configure the persistence partition at `device` for `kind`.
pub fn setup(device: &str, kind: PersistenceKind) -> Result<()> {
    emit::phase("Persistence");
    // The live system locates the overlay by this volume label.
    let label = match kind {
        PersistenceKind::CasperRw => "casper-rw",
        PersistenceKind::DebianLive => "persistence",
    };
    fsutil::mkfs_ext4(device, label)?;

    // Debian-live additionally needs a persistence.conf saying what to persist.
    if kind == PersistenceKind::DebianLive {
        let mount = fsutil::Mount::new(device, "ext4")?;
        let conf = mount.path().join("persistence.conf");
        std::fs::write(&conf, b"/ union\n")
            .with_context(|| format!("writing {}", conf.display()))?;
    }
    emit::log("Persistence partition created");
    Ok(())
}

/// Patch the copied bootloader configs on `target` so the live system
/// actually activates persistence — without the kernel option the overlay
/// partition is created but never used. Mirrors Rufus's `iso.c` patching.
pub fn patch_boot_config(target: &Path, kind: PersistenceKind) -> Result<()> {
    let (marker, replacement) = match kind {
        PersistenceKind::CasperRw => ("boot=casper", "boot=casper persistent"),
        PersistenceKind::DebianLive => ("boot=live", "boot=live persistence"),
    };
    let mut patched = 0u32;
    patch_dir(target, marker, replacement, &mut patched);
    emit::log(format!(
        "Enabled persistence in {patched} bootloader config file(s)"
    ));
    Ok(())
}

/// Recursively rewrite `*.cfg` / `*.conf` boot configs under `dir`, adding the
/// persistence kernel option. Best-effort: unreadable files are skipped.
fn patch_dir(dir: &Path, marker: &str, replacement: &str, patched: &mut u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            patch_dir(&path, marker, replacement, patched);
        } else if file_type.is_file() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !(name.ends_with(".cfg") || name.ends_with(".conf")) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains(marker) && !content.contains(replacement) {
                    if std::fs::write(&path, content.replace(marker, replacement)).is_ok() {
                        *patched += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casper_config_gains_the_persistent_keyword() {
        let dir = std::env::temp_dir().join(format!("usbooty-bootcfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("boot/grub")).unwrap();
        let cfg = dir.join("boot/grub/grub.cfg");
        std::fs::write(&cfg, "linux /casper/vmlinuz boot=casper quiet splash ---\n").unwrap();

        patch_boot_config(&dir, PersistenceKind::CasperRw).unwrap();
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("boot=casper persistent"));
        // Idempotent — a second pass must not double the keyword.
        patch_boot_config(&dir, PersistenceKind::CasperRw).unwrap();
        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap().matches("persistent").count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
