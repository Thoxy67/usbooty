//! Setting up a Linux live-USB persistence partition (the writable overlay
//! that lets changes survive a reboot).

use anyhow::{Context, Result};
use std::path::Path;

use usbooty_core::PersistenceKind;

use crate::{emit, fsutil};

/// The fixed volume label used for the Fedora overlay partition. dracut picks
/// the partition up at boot via the `rd.live.overlay=LABEL=…` kernel option.
const FEDORA_OVERLAY_LABEL: &str = "OVERLAY";
/// COW-store file that dracut's dmsquash-live loop-mounts on the Fedora /
/// RHEL-family overlay partition (a bare partition is not used directly).
const FEDORA_OVERLAY_FILE: &str = "overlay.img";
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
        PersistenceKind::SlaxChanges | PersistenceKind::AlpineLbu => {
            // Inline schemes (Slax, Alpine) have no dedicated partition; the
            // caller must route them through setup_inline() instead.
            anyhow::bail!("{kind:?} persistence is inline (no partition); call setup_inline()");
        }
    };
    fsutil::mkfs_ext4(device, label)?;

    match kind {
        // Debian-live needs a persistence.conf saying what to persist.
        PersistenceKind::DebianLive => {
            let mount = fsutil::Mount::new(device, "ext4")?;
            let conf = mount.path().join("persistence.conf");
            std::fs::write(&conf, b"/ union\n")
                .with_context(|| format!("writing {}", conf.display()))?;
        }
        // dracut's dmsquash-live (Fedora and the RHEL rebuilds) does not treat
        // a bare partition as the overlay: it loop-mounts a COW *file* on it as
        // a dm-snapshot. Create a zeroed sparse file filling the partition; the
        // kernel arg in patch_boot_config points dracut at it.
        //
        // NOTE: only this wiring is unit-tested. Actual persistence across a
        // reboot must be verified on real Fedora / AlmaLinux / Rocky / CentOS
        // live media.
        PersistenceKind::FedoraOverlay => {
            let mount = fsutil::Mount::new(device, "ext4")?;
            let path = mount.path().join(FEDORA_OVERLAY_FILE);
            let file = std::fs::File::create(&path)
                .with_context(|| format!("creating {}", path.display()))?;
            let stat = nix::sys::statvfs::statvfs(mount.path())
                .context("measuring the overlay partition")?;
            let free = stat.blocks_available() as u64 * stat.fragment_size() as u64;
            // Leave a little slack so the host filesystem keeps some headroom.
            let size = free.saturating_sub(16 * 1024 * 1024);
            file.set_len(size)
                .with_context(|| format!("allocating {}", path.display()))?;
        }
        _ => {}
    }
    emit::log("Persistence partition created");
    Ok(())
}

/// Inline-folder persistence: create the directory Slax saves changes into
/// directly on the main data partition. The kernel option that activates the
/// feature (`perch`) is added by [`patch_boot_config`] in the same flow, so
/// the user lands on a working persistent stick on first boot, no boot-menu
/// fiddling required.
pub fn setup_inline(data_mount: &Path, kind: PersistenceKind) -> Result<()> {
    match kind {
        PersistenceKind::SlaxChanges => {
            let dir = data_mount.join("slax").join("changes");
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            emit::log(format!(
                "Slax persistence directory created at {}",
                dir.display()
            ));
            Ok(())
        }
        PersistenceKind::AlpineLbu => {
            // Alpine persists via lbu (an apkovl tarball) on the writable boot
            // media. The only thing to prepare here is a local apk package
            // cache directory so `setup-apkcache` has a target; the apkovl
            // itself is written at runtime by `lbu commit`.
            let dir = data_mount.join("cache");
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            emit::log(format!(
                "Alpine apk cache directory created at {}",
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
/// actually activates persistence; without the kernel option the overlay
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
            // Point dracut at the COW file created in `setup` (the
            // devspec:pathspec form). Inserted after the `rd.live.image` marker
            // every Fedora / RHEL-family live cmdline carries.
            let kernel_arg =
                format!("rd.live.overlay=LABEL={FEDORA_OVERLAY_LABEL}:/{FEDORA_OVERLAY_FILE}");
            patch_dir(
                target,
                "rd.live.image",
                &format!("rd.live.image {kernel_arg}"),
                &mut patched,
            );
        }
        PersistenceKind::OpenSuseCow => {
            // kiwi-live creates its own persistent write partition (in free
            // space) when `rd.live.overlay.persistent` is on the cmdline; it
            // does not adopt a pre-made labelled partition. CAVEAT: this needs
            // unpartitioned free space, which the current cow-partition layout
            // does not leave, so this is only half the fix. Verify on real
            // openSUSE live media.
            patch_dir(
                target,
                "rd.live.image",
                "rd.live.image rd.live.overlay.persistent",
                &mut patched,
            );
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
            // Slax 9+ saves changes to /slax/changes/ automatically on writable
            // media, with no kernel parameter required (the only related arg,
            // `perchsize=`, merely raises the FAT 16 GiB cap). The directory is
            // created in setup_inline; there is nothing to patch here.
        }
        PersistenceKind::AlpineLbu => {
            // Alpine's diskless init auto-loads the apkovl from the writable
            // boot media; no kernel parameter is needed.
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
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.contains(marker)
                && !content.contains(replacement)
                && std::fs::write(&path, content.replace(marker, replacement)).is_ok()
            {
                *patched += 1;
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
        // Idempotent: a second pass must not double the keyword.
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
        // Idempotent: a second pass must not double the keyword.
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
