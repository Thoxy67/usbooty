//! Setting up a Linux live-USB persistence partition (the writable overlay
//! that lets changes survive a reboot).

use anyhow::{Context, Result};
use std::path::Path;

use usbooty_core::PersistenceKind;

use crate::{emit, fsutil};

/// The fixed volume label used for the Fedora overlay partition. dracut picks
/// the partition up at boot via the `rd.live.overlay=LABEL=…` kernel option.
const FEDORA_OVERLAY_LABEL: &str = "OVERLAY";
/// The fixed volume label kiwi-live (openSUSE) uses for its overlay partition.
/// The live system reads it by label automatically; no kernel arg needed.
const OPENSUSE_COW_LABEL: &str = "cow";
/// Label used for the archiso overlay partition. The Arch initramfs hook
/// (`archiso_loop_mnt`) mounts whichever partition matches `cow_label=…` on
/// the kernel command line; Rufus picked `PERSISTENCE` upstream and we match it
/// so a stick written by either tool is interchangeable.
const ARCH_COW_LABEL: &str = "PERSISTENCE";

/// Format and configure the persistence partition at `device` for `kind`.
///
/// Caller guarantees `kind.needs_partition()` is true; the [`SlaxChanges`]
/// variant uses [`setup_inline`] instead because it lives in the main
/// data partition.
///
/// [`SlaxChanges`]: usbooty_core::PersistenceKind::SlaxChanges
pub fn setup(device: &str, kind: PersistenceKind) -> Result<()> {
    emit::phase("Persistence");
    // The live system locates the overlay by this volume label.
    let label = match kind {
        PersistenceKind::CasperRw => "casper-rw",
        PersistenceKind::DebianLive => "persistence",
        PersistenceKind::FedoraOverlay => FEDORA_OVERLAY_LABEL,
        PersistenceKind::OpenSuseCow => OPENSUSE_COW_LABEL,
        PersistenceKind::ArchOverlay => ARCH_COW_LABEL,
        PersistenceKind::SlaxChanges => {
            // The caller violated the contract — route it through the
            // inline path instead of returning a useless empty partition.
            anyhow::bail!(
                "Slax persistence uses an inline directory, not a partition; \
                 call setup_inline() instead"
            );
        }
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

/// Inline-folder persistence: create the directory Slax saves changes into
/// directly on the main data partition. The kernel option that activates the
/// feature (`perch`) is added by [`patch_boot_config`] in the same flow, so
/// the user lands on a working persistent stick on first boot — no boot-menu
/// fiddling required.
pub fn setup_inline(data_mount: &Path, kind: PersistenceKind) -> Result<()> {
    match kind {
        PersistenceKind::SlaxChanges => {
            let dir = data_mount.join("slax").join("changes");
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating {}", dir.display()))?;
            emit::log(format!(
                "Slax persistence directory created at {}",
                dir.display()
            ));
            Ok(())
        }
        _ => anyhow::bail!(
            "{:?} is a partition-based persistence scheme; call setup() instead",
            kind
        ),
    }
}

/// Patch the copied bootloader configs on `target` so the live system
/// actually activates persistence — without the kernel option the overlay
/// partition is created but never used. Mirrors Rufus's `iso.c` patching.
pub fn patch_boot_config(target: &Path, kind: PersistenceKind) -> Result<()> {
    let mut patched = 0u32;
    match kind {
        PersistenceKind::CasperRw => patch_dir(
            target,
            "boot=casper",
            "boot=casper persistent",
            &mut patched,
        ),
        PersistenceKind::DebianLive => {
            patch_dir(target, "boot=live", "boot=live persistence", &mut patched)
        }
        PersistenceKind::FedoraOverlay => {
            // dracut activates the overlay when `rd.live.overlay=LABEL=<lbl>`
            // is on the kernel command line. Insert it right after the
            // `rd.live.image` marker the Fedora installer always ships.
            let kernel_arg = format!("rd.live.overlay=LABEL={FEDORA_OVERLAY_LABEL}");
            patch_dir(
                target,
                "rd.live.image",
                &format!("rd.live.image {kernel_arg}"),
                &mut patched,
            );
        }
        PersistenceKind::OpenSuseCow => {
            // No bootloader patching needed — kiwi-live finds the COW
            // partition by label at boot. The mkfs label set in `setup` is
            // the entire integration.
        }
        PersistenceKind::ArchOverlay => {
            // The archiso initramfs hook activates an overlay when
            // `cow_label=<LABEL>` is on the kernel command line. Every
            // archiso bootloader config (BIOS syslinux, UEFI systemd-boot,
            // GRUB loopback) already carries `archisobasedir=arch`, so use
            // that as the insertion anchor.
            let kernel_arg = format!("cow_label={ARCH_COW_LABEL}");
            patch_dir(
                target,
                "archisobasedir=arch",
                &format!("archisobasedir=arch {kernel_arg}"),
                &mut patched,
            );
        }
        PersistenceKind::SlaxChanges => {
            // Slax 9+ activates persistent changes when `perch` appears on
            // the kernel command line. Slax's syslinux/GRUB configs use
            // `from=/slax` as the canonical entry-point string; append
            // `perch` right after it so every menu entry (BIOS isolinux and
            // UEFI grub) gets the kernel arg.
            patch_dir(target, "from=/slax", "from=/slax perch", &mut patched);
        }
    }
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
    fn archiso_config_gains_the_cow_label() {
        let dir = std::env::temp_dir().join(format!("usbooty-archcfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("loader/entries")).unwrap();
        let cfg = dir.join("loader/entries/01-archiso-linux.conf");
        std::fs::write(
            &cfg,
            "options  archisobasedir=arch archisosearchuuid=2026-05-01-06-05-08-00\n",
        )
        .unwrap();

        patch_boot_config(&dir, PersistenceKind::ArchOverlay).unwrap();
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("archisobasedir=arch cow_label=PERSISTENCE"));
        // Idempotent — a second pass must not double the keyword.
        patch_boot_config(&dir, PersistenceKind::ArchOverlay).unwrap();
        assert_eq!(
            std::fs::read_to_string(&cfg)
                .unwrap()
                .matches("cow_label")
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

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
            std::fs::read_to_string(&cfg)
                .unwrap()
                .matches("persistent")
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
