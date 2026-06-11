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
/// Label the Knoppix initrd scans for and auto-adopts as its persistent
/// overlay; no kernel parameter is involved.
const KNOPPIX_DATA_LABEL: &str = "KNOPPIX-DATA";

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
        PersistenceKind::FedoraOverlay | PersistenceKind::FedoraOverlayFs => FEDORA_OVERLAY_LABEL,
        PersistenceKind::OpenSuseCow => OPENSUSE_COW_LABEL,
        PersistenceKind::ArchOverlay => ARCH_COW_LABEL,
        // The Knoppix initrd auto-adopts any partition with this label as
        // its persistent overlay (ext4 works for 8.x/9.x). Needs
        // verification on real Knoppix media; older releases used a
        // `knoppix.img` file instead.
        PersistenceKind::KnoppixData => KNOPPIX_DATA_LABEL,
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
            // Resolve the copied `slax/` case-insensitively so a case-sensitive
            // destination reuses it instead of creating a colliding sibling.
            // See [`crate::fsutil::ci_join`].
            let dir = crate::fsutil::ci_join(data_mount, &["slax", "changes"]);
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

/// A filename/argument character: an occurrence of an anchor followed by one
/// of these is a *longer* token (e.g. `/casper/vmlinuz` inside
/// `/casper/vmlinuz.efi`), not a match. `/` is deliberately not included so
/// prefix anchors like `file=/cdrom/preseed` keep matching the full
/// `file=/cdrom/preseed/ubuntu.seed` argument, as Rufus's anchors do.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Whether an occurrence of `marker` ending at byte `end` of `content` sits
/// on a token boundary. Markers that already end in a delimiter (such as the
/// `archisobasedir=` prefix anchors) match anywhere by design.
fn anchored_at(content: &str, marker: &str, end: usize) -> bool {
    if !marker.chars().next_back().is_some_and(is_token_char) {
        return true;
    }
    !content[end..].chars().next().is_some_and(is_token_char)
}

/// Whether `content` contains `marker` on a token boundary (see
/// [`anchored_at`]). A bare substring `contains` would also match inside a
/// longer token, e.g. the `/casper/vmlinuz` anchor inside
/// `/casper/vmlinuz.efi`, and the subsequent replace would corrupt that
/// kernel path into `/casper/vmlinuz persistent.efi`.
fn contains_anchored(content: &str, marker: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = content[from..].find(marker) {
        let start = from + pos;
        let end = start + marker.len();
        if anchored_at(content, marker, end) {
            return true;
        }
        from = end;
    }
    false
}

/// `content.replace(marker, replacement)` restricted to occurrences on a
/// token boundary; the counterpart of [`contains_anchored`].
fn replace_anchored(content: &str, marker: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(content.len() + replacement.len());
    let mut from = 0;
    while let Some(pos) = content[from..].find(marker) {
        let start = from + pos;
        let end = start + marker.len();
        out.push_str(&content[from..start]);
        if anchored_at(content, marker, end) {
            out.push_str(replacement);
        } else {
            out.push_str(marker);
        }
        from = end;
    }
    out.push_str(&content[from..]);
    out
}

/// An ordered anchor cascade: for each boot-config file, the *first* pair
/// whose marker is present is applied and the rest are skipped. Mirrors
/// Rufus's `iso.c` fallback chain, where one config syntax (say, Mint's
/// `boot=casper`) must win over a more generic anchor present in the same
/// file.
type PatchCascade<'a> = &'a [(&'a str, &'a str)];

/// Patch the copied bootloader configs on `target` so the live system
/// actually activates persistence; without the kernel option the overlay
/// partition is created but never used. Mirrors Rufus's `iso.c` patching.
pub fn patch_boot_config(target: &Path, kind: PersistenceKind) -> Result<()> {
    // Pre-formatted args for the cascades below (cascade entries are &str).
    let fedora_arg = format!(
        "rd.live.image rd.live.overlay=LABEL={FEDORA_OVERLAY_LABEL}:/{FEDORA_OVERLAY_FILE}"
    );
    let fedora_overlayfs_arg = format!(
        "rd.live.image rd.live.overlay=LABEL={FEDORA_OVERLAY_LABEL} rd.live.overlay.overlayfs=1"
    );
    let cow_arg_archiso = format!("cow_label={ARCH_COW_LABEL} archisobasedir=");
    let cow_arg_miso = format!("cow_label={ARCH_COW_LABEL} misobasedir=");

    // (cascade, tokens to scrub from patched files)
    let (cascade, scrub): (PatchCascade, &[&str]) = match kind {
        // Ubuntu-family. The anchor moved around across releases (Rufus
        // iso.c documents the archaeology): preseed for classic Ubuntu,
        // `boot=casper` for Mint (must come before the vmlinuz anchors,
        // Mint configs carry both), bare `/casper/vmlinuz` for Ubuntu
        // 23.04+ GRUB configs that dropped `boot=casper` entirely.
        // `maybe-ubiquity` is scrubbed like Rufus does, so the patched
        // entry boots to the live desktop instead of the installer prompt.
        PersistenceKind::CasperRw => (
            &[
                ("file=/cdrom/preseed", "persistent file=/cdrom/preseed"),
                ("boot=casper", "boot=casper persistent"),
                ("/casper/vmlinuz", "/casper/vmlinuz persistent"),
            ],
            &[" maybe-ubiquity"],
        ),
        PersistenceKind::DebianLive => (&[("boot=live", "boot=live persistence")], &[]),
        // Point dracut at the COW file created in `setup` (the
        // devspec:pathspec form). Inserted after the `rd.live.image` marker
        // every Fedora / RHEL-family live cmdline carries.
        PersistenceKind::FedoraOverlay => (&[("rd.live.image", fedora_arg.as_str())], &[]),
        // Fedora 40+ dracut: use the labelled partition directly as an
        // overlayfs upper dir. No COW file, no fixed-size exhaustion.
        // Needs verification on real Fedora live media.
        PersistenceKind::FedoraOverlayFs => {
            (&[("rd.live.image", fedora_overlayfs_arg.as_str())], &[])
        }
        PersistenceKind::OpenSuseCow => {
            // kiwi-live creates its own persistent write partition (in free
            // space) when `rd.live.overlay.persistent` is on the cmdline; it
            // does not adopt a pre-made labelled partition. CAVEAT: this needs
            // unpartitioned free space, which the current cow-partition layout
            // does not leave, so this is only half the fix. Verify on real
            // openSUSE live media.
            (
                &[("rd.live.image", "rd.live.image rd.live.overlay.persistent")],
                &[],
            )
        }
        // The archiso initramfs hook activates an overlay when
        // `cow_label=<LABEL>` is on the kernel command line. Anchor on the
        // basedir token *prefix* (value varies per distro: `arch`,
        // `manjaro`, `garuda`, ...) and insert before it, so one cascade
        // covers archiso and the miso/buildiso forks alike. Whether miso
        // still honours `cow_label=` needs verification on real Manjaro
        // media; archiso does.
        PersistenceKind::ArchOverlay => (
            &[
                ("archisobasedir=", cow_arg_archiso.as_str()),
                ("misobasedir=", cow_arg_miso.as_str()),
            ],
            &[],
        ),
        PersistenceKind::SlaxChanges => {
            // Slax 9+ saves changes to /slax/changes/ automatically on writable
            // media, with no kernel parameter required (the only related arg,
            // `perchsize=`, merely raises the FAT 16 GiB cap). The directory is
            // created in setup_inline; there is nothing to patch here.
            (&[], &[])
        }
        PersistenceKind::AlpineLbu => {
            // Alpine's diskless init auto-loads the apkovl from the writable
            // boot media; no kernel parameter is needed.
            (&[], &[])
        }
        PersistenceKind::KnoppixData => {
            // The Knoppix initrd scans every partition for the
            // `KNOPPIX-DATA` label on its own; no kernel parameter needed.
            (&[], &[])
        }
    };

    let mut patched = 0u32;
    patch_dir(target, cascade, scrub, &mut patched);
    if cascade.is_empty() {
        // Nothing to patch by design (inline / auto-scan schemes).
        return Ok(());
    }
    if patched == 0 {
        // The partition exists but nothing will activate it: that's a
        // persistence-less stick that looks configured. Don't fail the job
        // (the copy already succeeded and the fix is one manual kernel
        // arg), but make the problem impossible to miss.
        emit::warn(format!(
            "No bootloader config matched the {kind:?} persistence anchors; \
             persistence will NOT activate automatically. Add the \
             distribution's persistence kernel option manually at boot."
        ));
    } else {
        emit::log(format!(
            "Enabled persistence in {patched} bootloader config file(s)"
        ));
    }
    Ok(())
}

/// Recursively rewrite `*.cfg` / `*.conf` boot configs under `dir`, adding
/// the persistence kernel option. For each file the first cascade entry
/// whose marker appears wins; `scrub` tokens are removed from patched files.
/// Best-effort: unreadable files are skipped.
fn patch_dir(dir: &Path, cascade: PatchCascade, scrub: &[&str], patched: &mut u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            patch_dir(&path, cascade, scrub, patched);
        } else if file_type.is_file() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !(name.ends_with(".cfg") || name.ends_with(".conf")) {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (marker, replacement) in cascade {
                if !contains_anchored(&content, marker) {
                    continue;
                }
                // First matching anchor decides this file; if its
                // replacement is already present the file was patched on a
                // previous run (idempotence).
                if !content.contains(replacement) {
                    let mut new_content = replace_anchored(&content, marker, replacement);
                    for token in scrub {
                        new_content = new_content.replace(token, "");
                    }
                    if std::fs::write(&path, new_content).is_ok() {
                        *patched += 1;
                    }
                }
                break;
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
        assert!(out.contains("cow_label=PERSISTENCE archisobasedir=arch"));
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
    fn miso_config_gains_the_cow_label() {
        // Manjaro/Garuda (miso/buildiso forks) carry `misobasedir=<name>`
        // instead of `archisobasedir=`; the generic anchor must catch it.
        let dir = std::env::temp_dir().join(format!("usbooty-misocfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("boot/grub")).unwrap();
        let cfg = dir.join("boot/grub/grub.cfg");
        std::fs::write(
            &cfg,
            "linux /boot/vmlinuz-x86_64 misobasedir=manjaro misolabel=MANJARO quiet\n",
        )
        .unwrap();

        patch_boot_config(&dir, PersistenceKind::ArchOverlay).unwrap();
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("cow_label=PERSISTENCE misobasedir=manjaro"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modern_ubuntu_grub_without_boot_casper_is_patched() {
        // Ubuntu 23.04+ grub.cfg has no `boot=casper`; the bare
        // `/casper/vmlinuz` anchor must still land `persistent`, and the
        // `maybe-ubiquity` token must be scrubbed like Rufus does.
        let dir = std::env::temp_dir().join(format!("usbooty-ub2304-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("boot/grub")).unwrap();
        let cfg = dir.join("boot/grub/grub.cfg");
        std::fs::write(
            &cfg,
            "linux /casper/vmlinuz layerfs-path=minimal.standard.live.squashfs maybe-ubiquity quiet splash ---\n",
        )
        .unwrap();

        patch_boot_config(&dir, PersistenceKind::CasperRw).unwrap();
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("/casper/vmlinuz persistent"));
        assert!(!out.contains("maybe-ubiquity"));
        // Idempotent.
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

    #[test]
    fn mint_boot_casper_anchor_wins_over_vmlinuz_anchor() {
        // Mint configs carry BOTH `boot=casper` and `linux /casper/vmlinuz`;
        // the cascade must apply exactly one anchor (the boot=casper one).
        let dir = std::env::temp_dir().join(format!("usbooty-mintcfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("isolinux")).unwrap();
        let cfg = dir.join("isolinux/live.cfg");
        std::fs::write(
            &cfg,
            "kernel /casper/vmlinuz\nappend boot=casper initrd=/casper/initrd.lz quiet\n",
        )
        .unwrap();

        patch_boot_config(&dir, PersistenceKind::CasperRw).unwrap();
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(out.matches("persistent").count(), 1, "{out}");
        assert!(out.contains("boot=casper persistent"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn anchors_do_not_match_inside_longer_tokens() {
        // `/casper/vmlinuz` must not match inside `/casper/vmlinuz.efi`:
        // patching there would corrupt the kernel path into
        // `/casper/vmlinuz persistent.efi` and the stick would not boot.
        assert!(!contains_anchored(
            "linux /casper/vmlinuz.efi quiet",
            "/casper/vmlinuz"
        ));
        assert_eq!(
            replace_anchored(
                "linux /casper/vmlinuz.efi quiet",
                "/casper/vmlinuz",
                "/casper/vmlinuz persistent"
            ),
            "linux /casper/vmlinuz.efi quiet"
        );
        // Plain occurrences (followed by whitespace or end) still match.
        assert!(contains_anchored("linux /casper/vmlinuz quiet", "/casper/vmlinuz"));
        assert!(contains_anchored("linux /casper/vmlinuz", "/casper/vmlinuz"));
        // Prefix anchors that continue with `/` (deeper path) still match,
        // mirroring Rufus's `file=/cdrom/preseed` behaviour.
        assert!(contains_anchored(
            "file=/cdrom/preseed/ubuntu.seed boot=casper",
            "file=/cdrom/preseed"
        ));
        // Anchors ending in a delimiter are prefix anchors by design.
        assert!(contains_anchored(
            "cow_label=X archisobasedir=arch",
            "archisobasedir="
        ));
    }

    #[test]
    fn vmlinuz_efi_config_is_left_unpatched() {
        let dir = std::env::temp_dir().join(format!("usbooty-efi-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("boot/grub")).unwrap();
        let cfg = dir.join("boot/grub/grub.cfg");
        let original = "linux /casper/vmlinuz.efi quiet splash ---\n";
        std::fs::write(&cfg, original).unwrap();

        patch_boot_config(&dir, PersistenceKind::CasperRw).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), original);
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
