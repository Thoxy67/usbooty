//! Per-distribution post-copy quirk fixes.
//!
//! Mirrors Rufus's `iso.c` distro-specific patch table. After the generic
//! ISO copy has landed every file on the USB, this module walks the mount
//! point and applies whatever rewrites the detected family is known to
//! need. Every fix is idempotent: re-running it on an already-patched
//! filesystem changes nothing. Failures are non-fatal — a missing config
//! file just means the quirk doesn't apply here.
//!
//! The set of fixes implemented:
//!
//! - **Arch / Manjaro / EndeavourOS / CachyOS / Bazzite / GeckoLinux** —
//!   write a fallback `/EFI/BOOT/grub.cfg` that uses `search` + `configfile`
//!   to locate the real config by volume label. archiso variants and
//!   several openSUSE-derived distros ship a `grubx64.efi` compiled with a
//!   hardcoded `prefix=` that breaks when the ISO is copied onto a USB with
//!   a different partition layout (Rufus #691, #2105, GeckoLinux issue).
//!
//! - **Knoppix** — append `vga=normal nodma` to the isolinux append lines.
//!   Older Knoppix releases default to a high-resolution vesa mode that
//!   hangs on several modern Intel GPUs; the safer defaults match what
//!   Knoppix's own `failsafe` menu entry uses.

use anyhow::Result;
use std::path::Path;

use usbooty_core::DistroFamily;

use crate::emit;

/// Apply every fix registered for `family` to the just-populated mount at
/// `mount`. Caller passes the *raw* ISO volume label so the fallback grub
/// config can reference the same label the bootloader sees at boot.
pub fn apply(mount: &Path, family: DistroFamily, volume_label: &str) {
    let fixes = fixes_for(family);
    if fixes.is_empty() {
        return;
    }
    emit::phase("Applying distro fixes");
    emit::log(format!(
        "Detected {}; applying {} known quirk fix(es)",
        family.display(),
        fixes.len()
    ));
    for fix in fixes {
        if let Err(e) = fix(mount, volume_label) {
            // A best-effort fix that fails is logged but does not abort the
            // job — the ISO is still bootable in the common case, the user
            // just loses the workaround.
            emit::log(format!("Distro fix skipped: {e:#}"));
        }
    }
}

/// Return the list of fix routines associated with `family`. Plain function
/// pointers (rather than closures) keep the table easy to read and grep.
fn fixes_for(family: DistroFamily) -> Vec<fn(&Path, &str) -> Result<()>> {
    match family {
        // archiso variants share the same hardcoded-prefix problem.
        DistroFamily::Arch
        | DistroFamily::Manjaro
        | DistroFamily::EndeavourOs
        | DistroFamily::CachyOs => vec![write_efi_grub_redirect],
        // Bazzite uses a Fedora-derived EFI layout; the prefix is sometimes
        // baked to /EFI/fedora, which breaks on a copied USB. Same redirect.
        DistroFamily::Bazzite => vec![write_efi_grub_redirect],
        DistroFamily::GeckoLinux => vec![write_efi_grub_redirect],
        DistroFamily::Knoppix => vec![knoppix_safe_vga],
        // Everything else: no fix-table entries today. Add new arms here as
        // we identify per-distro quirks worth handling.
        DistroFamily::Unknown
        | DistroFamily::Ubuntu
        | DistroFamily::Mint
        | DistroFamily::Lmde
        | DistroFamily::Debian
        | DistroFamily::Fedora
        | DistroFamily::Nobara
        | DistroFamily::AlmaLinux
        | DistroFamily::Rocky
        | DistroFamily::CentOs
        | DistroFamily::Alpine
        | DistroFamily::OpenSuse
        | DistroFamily::Slax => Vec::new(),
    }
}

/// Write a generic `/EFI/BOOT/grub.cfg` that searches every partition for
/// the real config by volume label and chainloads it. Only runs when the
/// file is missing — distros that ship their own EFI fallback config are
/// left untouched.
fn write_efi_grub_redirect(mount: &Path, volume_label: &str) -> Result<()> {
    let efi_boot = mount.join("EFI").join("BOOT");
    if !efi_boot.is_dir() {
        // Some derivatives use a lower-case `efi/boot` directory; ISO9660
        // collapses to upper case but ext4/UDF preserve the original. Try
        // both before giving up.
        let lower = mount.join("efi").join("boot");
        if lower.is_dir() {
            return write_efi_grub_redirect_at(&lower, volume_label);
        }
        anyhow::bail!("no /EFI/BOOT directory found — skipping grub redirect");
    }
    write_efi_grub_redirect_at(&efi_boot, volume_label)
}

/// Concrete writer used by both the case-folded and case-preserving paths.
fn write_efi_grub_redirect_at(efi_boot: &Path, volume_label: &str) -> Result<()> {
    let cfg_path = efi_boot.join("grub.cfg");
    if cfg_path.is_file() {
        // The ISO already ships a fallback config; don't shadow it. The
        // common case where this still helps is when the prefix-baked
        // `grubx64.efi` ignores its own EFI/BOOT/grub.cfg and tries a
        // hardcoded path; if that's the case the user can rename the
        // shipped file by hand. The conservative default is safer.
        return Ok(());
    }
    // The search command first matches by label (when usbooty preserved the
    // ISO's label) and falls back to a fuzzy any-EFI search so a relabelled
    // USB still boots. `--no-floppy` mimics what GRUB's stock fallback uses.
    let safe_label = volume_label.replace('"', "");
    let cfg = format!(
        "# Generated by usbooty as a safe-boot redirect for distros whose\n\
         # signed EFI bootloader hard-codes the wrong `prefix=` path. The\n\
         # snippet finds the partition holding the real GRUB config by\n\
         # volume label and chainloads it.\n\
         search --no-floppy --set=root --label \"{safe_label}\"\n\
         if [ -f \"/boot/grub/grub.cfg\" ]; then\n  \
             configfile /boot/grub/grub.cfg\n\
         elif [ -f \"/EFI/BOOT/grub.cfg.real\" ]; then\n  \
             configfile /EFI/BOOT/grub.cfg.real\n\
         elif [ -f \"/boot/grub2/grub.cfg\" ]; then\n  \
             configfile /boot/grub2/grub.cfg\n\
         fi\n"
    );
    std::fs::write(&cfg_path, cfg.as_bytes())?;
    emit::log(format!("Wrote EFI grub redirect at {}", cfg_path.display()));
    Ok(())
}

/// Append `vga=normal nodma` to every `APPEND` / `linux` line in Knoppix's
/// isolinux configs that doesn't already carry them. Older Knoppix releases
/// hang on several Intel/AMD GPUs in their default vesa mode; the failsafe
/// menu uses these flags, so we promote them to the default boot. Idempotent.
fn knoppix_safe_vga(mount: &Path, _volume_label: &str) -> Result<()> {
    let mut patched = 0u32;
    patch_config_lines(mount, &mut patched, |line| {
        let trimmed = line.trim_start();
        let is_append = trimmed.to_ascii_lowercase().starts_with("append ");
        let is_linux = trimmed.to_ascii_lowercase().starts_with("linux ");
        if !(is_append || is_linux) {
            return None;
        }
        if line.contains("vga=normal") && line.contains("nodma") {
            return None;
        }
        let mut out = line.trim_end().to_string();
        if !out.contains("vga=normal") {
            out.push_str(" vga=normal");
        }
        if !out.contains("nodma") {
            out.push_str(" nodma");
        }
        Some(out)
    });
    if patched > 0 {
        emit::log(format!(
            "Knoppix safe-boot flags added to {patched} bootloader entry/entries"
        ));
    }
    Ok(())
}

/// Walk every `*.cfg` / `*.conf` file under `dir` and pass each line through
/// `rewrite`. Lines for which the callback returns `Some(replacement)` are
/// rewritten; everything else is preserved verbatim. Returns the count of
/// modified lines via `patched`.
fn patch_config_lines(
    dir: &Path,
    patched: &mut u32,
    rewrite: impl Fn(&str) -> Option<String> + Copy,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            patch_config_lines(&path, patched, rewrite);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !(name.ends_with(".cfg") || name.ends_with(".conf")) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut changed = false;
        let mut rebuilt = String::with_capacity(content.len());
        for line in content.split_inclusive('\n') {
            let stripped = line.strip_suffix('\n').unwrap_or(line);
            match rewrite(stripped) {
                Some(new) if new != stripped => {
                    rebuilt.push_str(&new);
                    if line.ends_with('\n') {
                        rebuilt.push('\n');
                    }
                    *patched += 1;
                    changed = true;
                }
                _ => rebuilt.push_str(line),
            }
        }
        if changed {
            let _ = std::fs::write(&path, rebuilt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("usbooty-distrofix-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn arch_writes_efi_grub_redirect() {
        let mount = make_tree("arch");
        std::fs::create_dir_all(mount.join("EFI/BOOT")).unwrap();
        apply(&mount, DistroFamily::Arch, "ARCH_202605");
        let cfg = std::fs::read_to_string(mount.join("EFI/BOOT/grub.cfg")).unwrap();
        assert!(cfg.contains("search --no-floppy --set=root --label"));
        assert!(cfg.contains("ARCH_202605"));
        // Re-running is a no-op.
        let before = std::fs::read_to_string(mount.join("EFI/BOOT/grub.cfg")).unwrap();
        apply(&mount, DistroFamily::Arch, "ARCH_202605");
        let after = std::fs::read_to_string(mount.join("EFI/BOOT/grub.cfg")).unwrap();
        assert_eq!(before, after, "fix must be idempotent");
        let _ = std::fs::remove_dir_all(&mount);
    }

    #[test]
    fn existing_efi_grub_cfg_is_kept() {
        let mount = make_tree("preserve");
        std::fs::create_dir_all(mount.join("EFI/BOOT")).unwrap();
        std::fs::write(mount.join("EFI/BOOT/grub.cfg"), b"shipped config\n").unwrap();
        apply(&mount, DistroFamily::Bazzite, "BAZZITE");
        assert_eq!(
            std::fs::read_to_string(mount.join("EFI/BOOT/grub.cfg")).unwrap(),
            "shipped config\n"
        );
        let _ = std::fs::remove_dir_all(&mount);
    }

    #[test]
    fn knoppix_adds_safe_vga_flags() {
        let mount = make_tree("knoppix");
        std::fs::create_dir_all(mount.join("boot/isolinux")).unwrap();
        let cfg = mount.join("boot/isolinux/isolinux.cfg");
        std::fs::write(
            &cfg,
            "label knoppix\n  kernel /boot/vmlinuz\n  append initrd=/boot/initrd ramdisk_size=100000\n",
        )
        .unwrap();
        apply(&mount, DistroFamily::Knoppix, "KNOPPIX");
        let out = std::fs::read_to_string(&cfg).unwrap();
        assert!(out.contains("vga=normal"));
        assert!(out.contains("nodma"));
        // Idempotent: a second pass doesn't double the flags.
        apply(&mount, DistroFamily::Knoppix, "KNOPPIX");
        let out2 = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(out2.matches("vga=normal").count(), 1);
        assert_eq!(out2.matches("nodma").count(), 1);
        let _ = std::fs::remove_dir_all(&mount);
    }

    #[test]
    fn unknown_family_is_a_no_op() {
        let mount = make_tree("unknown");
        std::fs::create_dir_all(mount.join("EFI/BOOT")).unwrap();
        apply(&mount, DistroFamily::Unknown, "LABEL");
        assert!(!mount.join("EFI/BOOT/grub.cfg").exists());
        let _ = std::fs::remove_dir_all(&mount);
    }
}
