//! ISO image analysis.
//!
//! Reads the ISO9660 filesystem directly (via `cdfs`, with Joliet/Rock Ridge
//! support) — no mounting, no root — and classifies the image the way Rufus's
//! `iso.c` does: Windows vs Linux, and whether `install.wim` is too large for
//! plain FAT32.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use cdfs::{DirectoryEntry, ISO9660};
use usbooty_core::{IsoReport, OsKind};

/// FAT32's single-file ceiling, used for the `has_4gb_file` flag.
const FOUR_GIB: u64 = 0xFFFF_FFFF;

/// Analyze the ISO at `path`. Never fails: an unreadable or exotic image still
/// yields a usable (if minimal) report, since it can still be DD-written.
pub fn analyze(path: &Path) -> IsoReport {
    let total_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut report = IsoReport::unknown(total_size);

    report.is_isohybrid = detect_isohybrid(path);
    report.label = read_volume_label(path);

    let Ok(file) = File::open(path) else {
        return report;
    };
    let Ok(iso) = ISO9660::new(file) else {
        return report; // not an ISO9660 image — DD is still fine
    };

    // Root-level marker files.
    if let Some(root) = list_dir(&iso, &[]) {
        for (name, _is_dir, _size) in &root {
            match name.as_str() {
                "bootmgr" => report.has_bootmgr = true,
                "setup.exe" => report.has_setup_exe = true,
                _ => {}
            }
        }
    }

    // Windows install image. ISO9660 splits files >4 GiB into several
    // directory records ("multi-extent"); cdfs surfaces each as its own entry,
    // so summing every record with the same name yields the true size.
    if let Some(sources) = list_dir(&iso, &["sources"]) {
        let mut wim_size = 0u64;
        for (name, _is_dir, size) in &sources {
            if name == "install.wim" || name == "install.esd" || name == "install.swm" {
                report.has_install_wim = true;
                report.install_wim_is_esd |= name == "install.esd";
                wim_size += size;
            }
        }
        if report.has_install_wim {
            report.install_wim_size = Some(wim_size);
        }
    }

    // UEFI bootloader directory: /EFI/BOOT/boot*.efi.
    if let Some(efi_boot) = list_dir(&iso, &["efi", "boot"]) {
        report.has_efi_boot_dir = efi_boot
            .iter()
            .any(|(name, _, _)| name.starts_with("boot") && name.ends_with(".efi"));
    }

    // Linux bootloaders.
    report.has_isolinux =
        list_dir(&iso, &["isolinux"]).is_some() || list_dir(&iso, &["boot", "isolinux"]).is_some();
    report.has_grub =
        list_dir(&iso, &["boot", "grub"]).is_some() || list_dir(&iso, &["boot", "grub2"]).is_some();

    // Largest single (possibly multi-extent) file >= 4 GiB.
    report.has_4gb_file = report.install_wim_size.is_some_and(|s| s >= FOUR_GIB);

    report.os_kind = classify(&report);

    // Modern Windows ISOs are UDF images carrying only a near-empty ISO9660
    // stub, which `cdfs` (used above) cannot see into. Detect the UDF
    // filesystem directly: unless the disc was clearly identified as Linux,
    // treat it as a Windows installer with an oversized install.wim, so the
    // partition method routes it to the NTFS / split layout rather than
    // failing to fit it on FAT32. The helper loop-mounts the ISO for real.
    if report.os_kind != OsKind::Linux && detect_udf(path) {
        report.os_kind = OsKind::Windows;
        report.has_install_wim = true;
        report.has_4gb_file = true;
        if report.install_wim_size.map_or(true, |s| s <= FOUR_GIB) {
            report.install_wim_size = Some(FOUR_GIB + 1);
        }
    }

    report
}

/// Read the volume label from the ISO9660 Primary Volume Descriptor.
///
/// The volume descriptors begin at sector 16; the PVD (type byte 1) carries a
/// 32-byte, space-padded volume identifier at offset 40. This is present even
/// on UDF Windows ISOs, whose PVD label looks like `CCCOMA_X64FRE_EN-US_DV9`.
fn read_volume_label(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let mut sector = [0u8; 2048];
    for i in 16..32u64 {
        if file.seek(SeekFrom::Start(i * 2048)).is_err() || file.read_exact(&mut sector).is_err() {
            break;
        }
        if &sector[1..6] != b"CD001" || sector[0] == 0xFF {
            break; // end of the volume descriptor set
        }
        if sector[0] == 1 {
            return String::from_utf8_lossy(&sector[40..72])
                .trim_matches(|c: char| c == ' ' || c == '\0')
                .to_string();
        }
    }
    String::new()
}

/// Detect a UDF filesystem by scanning the ISO Volume Recognition Sequence —
/// a run of 2048-byte descriptors starting at sector 16. An `NSR0x` descriptor
/// means UDF is present; `TEA01` terminates the sequence.
fn detect_udf(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::Start(16 * 2048)).is_err() {
        return false;
    }
    let mut sector = [0u8; 2048];
    for _ in 0..16 {
        if file.read_exact(&mut sector).is_err() {
            return false;
        }
        match &sector[1..6] {
            b"NSR02" | b"NSR03" => return true,
            b"TEA01" => return false,
            _ => {}
        }
    }
    false
}

/// Apply Rufus-style classification rules to a populated report.
fn classify(report: &IsoReport) -> OsKind {
    if report.has_install_wim && (report.has_bootmgr || report.has_setup_exe) {
        OsKind::Windows
    } else if report.has_isolinux || report.has_grub {
        OsKind::Linux
    } else {
        OsKind::Other
    }
}

/// Read an ISO9660 directory case-insensitively and return `(name, is_dir,
/// size)` for each child. `segments` is the path from the root; an empty slice
/// is the root directory itself. Returns `None` if the path is not a directory.
fn list_dir(iso: &ISO9660<File>, segments: &[&str]) -> Option<Vec<(String, bool, u64)>> {
    // Resolve the path one segment at a time, matching names ignoring case.
    let mut current = iso.open("/").ok()??;
    for segment in segments {
        let DirectoryEntry::Directory(dir) = &current else {
            return None;
        };
        let next = dir
            .contents()
            .flatten()
            .find(|entry| entry.identifier().eq_ignore_ascii_case(segment))?;
        current = next;
    }

    let DirectoryEntry::Directory(dir) = current else {
        return None;
    };
    let mut out = Vec::new();
    for entry in dir.contents().flatten() {
        let name = entry.identifier().to_ascii_lowercase();
        match entry {
            DirectoryEntry::Directory(_) => out.push((name, true, 0)),
            DirectoryEntry::File(f) => out.push((name, false, u64::from(f.size()))),
            DirectoryEntry::Symlink(_) => out.push((name, false, 0)),
        }
    }
    Some(out)
}

/// Detect an isohybrid image: a valid MBR signature plus a non-empty partition
/// table in the ISO's first sector. Such images are directly DD-bootable.
fn detect_isohybrid(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut mbr = [0u8; 512];
    if file.seek(SeekFrom::Start(0)).is_err() || file.read_exact(&mut mbr).is_err() {
        return false;
    }
    // 0x55AA boot signature, and at least one partition entry with a type.
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        return false;
    }
    (0..4).any(|i| mbr[446 + i * 16 + 4] != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Build an ISO from `(relative_path, contents)` pairs using `xorriso`.
    /// Returns `None` if `xorriso` is not installed (so CI without it skips).
    fn build_iso(files: &[(&str, &[u8])]) -> Option<(tempdir::Holder, std::path::PathBuf)> {
        if Command::new("xorriso").arg("-version").output().is_err() {
            return None;
        }
        let dir = tempdir::Holder::new();
        for (rel, contents) in files {
            let full = dir.path().join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        let iso = dir.path().join("out.iso");
        let ok = Command::new("xorriso")
            .args(["-as", "mkisofs", "-J", "-R", "-o"])
            .arg(&iso)
            .arg(dir.path())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        ok.then_some((dir, iso))
    }

    #[test]
    fn classifies_a_windows_iso() {
        let Some((_dir, iso)) = build_iso(&[
            ("sources/install.wim", b"MSWIM\0\0\0fake"),
            ("bootmgr", b"fake bootmgr"),
            ("setup.exe", b"fake setup"),
        ]) else {
            eprintln!("skipping: xorriso not installed");
            return;
        };
        let report = analyze(&iso);
        assert_eq!(report.os_kind, OsKind::Windows);
        assert!(report.has_install_wim);
        assert!(report.has_bootmgr);
        assert_eq!(report.install_wim_size, Some(12));
        assert!(!report.has_4gb_file);
    }

    #[test]
    fn classifies_a_linux_iso() {
        let Some((_dir, iso)) = build_iso(&[
            ("isolinux/isolinux.cfg", b"default linux"),
            ("isolinux/isolinux.bin", b"fake"),
            ("boot/grub/grub.cfg", b"menuentry {}"),
        ]) else {
            eprintln!("skipping: xorriso not installed");
            return;
        };
        let report = analyze(&iso);
        assert_eq!(report.os_kind, OsKind::Linux);
        assert!(report.has_isolinux);
        assert!(report.has_grub);
        assert!(!report.has_install_wim);
    }

    /// Minimal scoped temp directory helper (avoids an extra dependency).
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct Holder(PathBuf);

        impl Holder {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static COUNTER: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "usbooty-isotest-{}-{}",
                    std::process::id(),
                    COUNTER.fetch_add(1, Ordering::SeqCst),
                ));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).unwrap();
                Holder(path)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Holder {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
