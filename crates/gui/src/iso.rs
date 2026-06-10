//! ISO image analysis.
//!
//! Reads the ISO9660 filesystem directly (via `cdfs`, with Joliet/Rock Ridge
//! support) (no mounting, no root) and classifies the image the way Rufus's
//! `iso.c` does: Windows vs Linux, and whether `install.wim` is too large for
//! plain FAT32.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use cdfs::{DirectoryEntry, ISO9660};
use usbooty_core::revocation;
use usbooty_core::{DistroFamily, IsoReport, OsKind};

/// FAT32's single-file ceiling, used for the `has_4gb_file` flag.
const FOUR_GIB: u64 = 0xFFFF_FFFF;

/// Upper bound on how much of an `.efi` binary we read for the SBAT
/// revocation scan. Signed bootloaders are well under this; the cap stops a
/// corrupt or hostile ISO whose directory record claims a multi-GB file size
/// from pinning that much memory.
const MAX_EFI_SCAN_BYTES: u64 = 16 * 1024 * 1024;

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
        return report; // not an ISO9660 image; DD is still fine
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

    // Linux distro family: drives persistence routing and the per-distro
    // post-copy fix table. Mirrors Rufus's `iso.c` quirk table; see
    // `DistroFamily::detect` in `usbooty-core` for the full cascade.
    if report.os_kind == OsKind::Linux {
        let root = list_dir(&iso, &[]).unwrap_or_default();
        report.distro = DistroFamily::detect(&report.label, &root);
        report.persistence = report.distro.persistence();
        // The Fedora / RHEL overlay scheme only applies to real LIVE media (a
        // `LiveOS/` squashfs). Rocky / AlmaLinux / CentOS also ship install
        // DVDs that carry the same volume label but no live image, so don't
        // offer persistence for those.
        if report.persistence == Some(usbooty_core::PersistenceKind::FedoraOverlay)
            && !root
                .iter()
                .any(|(name, is_dir, _)| *is_dir && name.eq_ignore_ascii_case("liveos"))
        {
            report.persistence = None;
        }
    }

    // Modern Windows ISOs are UDF images carrying only a near-empty ISO9660
    // stub, which `cdfs` (used above) cannot see into. Re-open the file as a
    // UDF image and *actually walk* the tree, populating every field that
    // the ISO9660 pass left empty. Falls back to the heuristic ("assume
    // Windows with oversized install.wim") only when UDF parsing fails.
    if report.os_kind != OsKind::Linux && detect_udf(path) {
        if let Ok(file) = File::open(path) {
            if let Some(mut udf) = usbooty_core::udf::UdfFs::open(file) {
                augment_from_udf(&mut udf, &mut report);
            } else {
                fall_back_to_windows(&mut report);
            }
        } else {
            fall_back_to_windows(&mut report);
        }
    }

    // A BSD ISO carries no Linux bootloader directories, so it lands in
    // `Other`; recognize it by its volume label so the UI names it correctly
    // and steers the user to the (OS-agnostic) DD method.
    if report.os_kind == OsKind::Other && report.label.to_lowercase().contains("bsd") {
        report.os_kind = OsKind::Bsd;
    }

    // SBAT-based revocation scan of the ISO's EFI bootloaders. Best-effort:
    // failures (unreadable file, not a PE, no .sbat section) are silent.
    if report.has_efi_boot_dir {
        report.revocation_warnings = scan_efi_revocations(&iso);
        // ISO9660 pass missed the EFI dir on a UDF image; try UDF for SBAT.
        if report.revocation_warnings.is_empty()
            && let Ok(file) = File::open(path)
            && let Some(mut udf) = usbooty_core::udf::UdfFs::open(file)
        {
            report.revocation_warnings = scan_efi_revocations_udf(&mut udf);
        }
    } else if let Ok(file) = File::open(path) {
        // ISO9660 pass said no EFI dir, but it might be UDF-only. Try UDF.
        if let Some(mut udf) = usbooty_core::udf::UdfFs::open(file)
            && udf.list(&["efi", "boot"]).is_some()
        {
            report.has_efi_boot_dir = true;
            report.revocation_warnings = scan_efi_revocations_udf(&mut udf);
        }
    }

    report
}

/// Fill in everything the ISO9660 pass missed by walking the UDF tree. The
/// pass is best-effort: any sub-step that fails leaves that part of the
/// report at its previous (ISO9660-derived) value.
fn augment_from_udf(udf: &mut usbooty_core::udf::UdfFs<File>, report: &mut IsoReport) {
    // Volume label: keep the ISO9660 label if it's non-empty (it usually is
    // even on UDF discs, since the stub PVD carries the same name); fall back
    // to the UDF label otherwise.
    if report.label.is_empty() {
        let label = udf.label().to_string();
        if !label.is_empty() {
            report.label = label;
        }
    }

    // Root-level marker files (`bootmgr` / `setup.exe`).
    if let Some(root) = udf.list(&[]) {
        for entry in &root {
            let name = entry.name.to_ascii_lowercase();
            match name.as_str() {
                "bootmgr" => report.has_bootmgr = true,
                "setup.exe" => report.has_setup_exe = true,
                _ => {}
            }
        }
    }

    // Windows install image under /sources/. The FID's `icb.length` is the
    // length of the File Entry descriptor itself (one sector), not the file
    // size, so look up `information_length` from each matching FE.
    if let Some(sources) = udf.list(&["sources"]) {
        let names: Vec<String> = sources
            .iter()
            .map(|e| e.name.to_ascii_lowercase())
            .filter(|n| n == "install.wim" || n == "install.esd" || n == "install.swm")
            .collect();
        let mut wim_size = 0u64;
        for name in &names {
            report.has_install_wim = true;
            report.install_wim_is_esd |= name == "install.esd";
            wim_size += udf.file_size(&["sources", name]).unwrap_or(0);
        }
        if report.has_install_wim {
            report.install_wim_size = Some(wim_size);
            report.has_4gb_file = wim_size > FOUR_GIB;
        }
    }

    // UEFI bootloader directory.
    if let Some(efi_boot) = udf.list(&["efi", "boot"]) {
        report.has_efi_boot_dir = efi_boot
            .iter()
            .any(|e| e.name.to_ascii_lowercase().ends_with(".efi"));
    }

    // Classify now that the install.wim / bootmgr / setup.exe flags are set.
    report.os_kind = if report.has_install_wim && (report.has_bootmgr || report.has_setup_exe) {
        OsKind::Windows
    } else if report.has_isolinux || report.has_grub {
        OsKind::Linux
    } else {
        report.os_kind
    };
}

/// Fallback when UDF parsing fails entirely: keep the legacy heuristic so
/// the user still lands on the NTFS / split layout for what is almost
/// certainly a Windows installer.
fn fall_back_to_windows(report: &mut IsoReport) {
    report.os_kind = OsKind::Windows;
    report.has_install_wim = true;
    report.has_4gb_file = true;
    if report.install_wim_size.is_none_or(|s| s <= FOUR_GIB) {
        report.install_wim_size = Some(FOUR_GIB + 1);
    }
}

/// SBAT scan against EFI binaries surfaced by the UDF walker.
fn scan_efi_revocations_udf(udf: &mut usbooty_core::udf::UdfFs<File>) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(entries) = udf.list(&["efi", "boot"]) else {
        return warnings;
    };
    let db = crate::resources::cached_revocation_db();
    let names: Vec<String> = entries
        .into_iter()
        .filter(|e| !e.is_dir && e.name.to_ascii_lowercase().ends_with(".efi"))
        .map(|e| e.name)
        .collect();
    for name in names {
        let Some(bytes) = udf.read_file(&["efi", "boot", &name]) else {
            continue;
        };
        let Some(section) = revocation::extract_sbat(&bytes) else {
            continue;
        };
        let parsed = revocation::parse_sbat_section(&section);
        let lower = name.to_ascii_lowercase();
        for w in revocation::evaluate_sbat(&parsed, &db.sbat) {
            warnings.push(format!("{lower}: {w}"));
        }
    }
    warnings
}

/// Walk `/EFI/BOOT/*.efi` and warn for any binary whose `.sbat` section
/// reports a generation lower than the baked-in required level. Uses the same
/// case-insensitive directory walk as [`list_dir`] so it works on ISO9660
/// (upper-case) and UDF (case-preserving) media alike. Best-effort: any
/// failure (unreadable file, not a PE, no `.sbat`) is silent.
fn scan_efi_revocations(iso: &ISO9660<File>) -> Vec<String> {
    // Resolve /EFI/BOOT case-insensitively and yield each child file directly,
    // since `iso.open(path)` may not match arbitrary case combinations.
    let mut warnings = Vec::new();
    let db = crate::resources::cached_revocation_db();

    let Ok(Some(root)) = iso.open("/") else {
        return warnings;
    };
    let DirectoryEntry::Directory(root_dir) = root else {
        return warnings;
    };
    let Some(efi) = root_dir
        .contents()
        .flatten()
        .find(|e| e.identifier().eq_ignore_ascii_case("efi"))
    else {
        return warnings;
    };
    let DirectoryEntry::Directory(efi_dir) = efi else {
        return warnings;
    };
    let Some(boot) = efi_dir
        .contents()
        .flatten()
        .find(|e| e.identifier().eq_ignore_ascii_case("boot"))
    else {
        return warnings;
    };
    let DirectoryEntry::Directory(boot_dir) = boot else {
        return warnings;
    };

    for entry in boot_dir.contents().flatten() {
        let DirectoryEntry::File(file) = entry else {
            continue;
        };
        let name = file.identifier.to_ascii_lowercase();
        if !name.ends_with(".efi") {
            continue;
        }
        // Trust neither the reserve hint nor the stream length: cap both at
        // MAX_EFI_SCAN_BYTES so a bogus directory size can't drive a huge
        // allocation or an unbounded read.
        let cap = u64::from(file.size()).min(MAX_EFI_SCAN_BYTES);
        let mut bytes = Vec::with_capacity(cap as usize);
        if file
            .read()
            .take(MAX_EFI_SCAN_BYTES)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        let Some(section) = revocation::extract_sbat(&bytes) else {
            continue;
        };
        let entries = revocation::parse_sbat_section(&section);
        for w in revocation::evaluate_sbat(&entries, &db.sbat) {
            warnings.push(format!("{name}: {w}"));
        }
    }
    warnings
}

/// Read and decode the trailing `<WIM>` XML metadata of the ISO's
/// `install.wim` / `install.esd` (no full extraction, cheap even on a
/// multi-gigabyte image). `None` when the ISO is not UDF, has no install image,
/// or the WIM header is unrecognizable.
fn read_wim_xml(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut udf = usbooty_core::udf::UdfFs::open(file)?;
    let segs: &[&str] = if udf.file_size(&["sources", "install.wim"]).is_some() {
        &["sources", "install.wim"]
    } else if udf.file_size(&["sources", "install.esd"]).is_some() {
        &["sources", "install.esd"]
    } else {
        return None;
    };

    // WIM header (WIMHEADER_V1_PACKED): the XML-data resource header sits at
    // byte 0x48: 24 bytes of { size+flags (u64), offset (i64), originalSize }.
    let header = udf.read_file_range(segs, 0, 208).unwrap_or_default();
    if header.len() < 0x60 || &header[0..5] != b"MSWIM" {
        return None;
    }
    let size = u64::from_le_bytes(header[0x48..0x50].try_into().unwrap()) & 0x00FF_FFFF_FFFF_FFFF;
    let offset = u64::from_le_bytes(header[0x50..0x58].try_into().unwrap());
    if size == 0 || size > 64 * 1024 * 1024 {
        return None;
    }
    let xml_bytes = udf.read_file_range(segs, offset, size).unwrap_or_default();
    Some(decode_utf16le(&xml_bytes))
}

/// Windows install-image metadata derived from the WIM XML.
#[derive(Clone, Debug, Default)]
pub struct WindowsMeta {
    /// Build number (e.g. 26100), or 0 when it can't be determined. Used to
    /// gate version-specific installer customization options by Windows
    /// version, the way Rufus does (Windows 11 is build >= 22000; the
    /// Microsoft-account-bypass option, >= 22500).
    pub build: u32,
    /// Unattend processor-architecture name (`"x86"`, `"amd64"`, `"arm64"`),
    /// or `None` if undetermined. Lets the unattend emit a single
    /// arch-matched `<component>` instead of one per architecture.
    pub arch: Option<String>,
}

/// Read the install image's build number and architecture in one pass.
/// Parsing the WIM XML re-opens the file and walks the UDF descriptors,
/// which is disk-bound; call this from a worker thread, once per ISO.
pub fn windows_meta(path: &Path) -> WindowsMeta {
    match read_wim_xml(path) {
        Some(xml) => WindowsMeta {
            build: parse_windows_build(&xml),
            arch: parse_windows_arch(&xml),
        },
        None => WindowsMeta::default(),
    }
}

/// Decode a UTF-16LE byte buffer (with an optional leading BOM) to a `String`,
/// replacing invalid units rather than failing.
fn decode_utf16le(bytes: &[u8]) -> String {
    let start = if bytes.starts_with(&[0xFF, 0xFE]) {
        2
    } else {
        0
    };
    let units: Vec<u16> = bytes[start..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

/// Pull the first `<BUILD>NNNNN</BUILD>` value out of the WIM `<WIM>` XML
/// metadata (every `<IMAGE>` in one `install.wim` shares the same build).
fn parse_windows_build(xml: &str) -> u32 {
    let Some(start) = xml.find("<BUILD>") else {
        return 0;
    };
    let rest = &xml[start + "<BUILD>".len()..];
    let Some(end) = rest.find("</BUILD>") else {
        return 0;
    };
    rest[..end].trim().parse().unwrap_or(0)
}

/// Map the first `<ARCH>N</ARCH>` (the `PROCESSOR_ARCHITECTURE` value Windows
/// stores in the WIM XML) to an unattend `processorArchitecture` name. Returns
/// `None` for unknown / unhandled values so the caller falls back to emitting
/// every architecture.
fn parse_windows_arch(xml: &str) -> Option<String> {
    let start = xml.find("<ARCH>")? + "<ARCH>".len();
    let rest = &xml[start..];
    let end = rest.find("</ARCH>")?;
    match rest[..end].trim() {
        "0" => Some("x86".into()),
        "9" => Some("amd64".into()),
        "12" => Some("arm64".into()),
        _ => None,
    }
}

/// The set of digests usbooty computes for a source ISO.
///
/// Distros and Microsoft publish hashes in a variety of formats (some still
/// only MD5, some SHA-256, the most cautious publish SHA-512), so the GUI
/// surfaces every common digest. BLAKE3 is kept too because the helper uses it
/// internally for fast read-back verification and showing it gives the user
/// the same value to cross-check the post-write integrity report against.
///
/// Each field is a lowercase hex string, empty when the ISO could not be read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IsoHashes {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub sha512: String,
    pub blake3: String,
}

/// Identifies which digest a [`compute_hashes`] per-hash callback is reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashKind {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Blake3,
}

/// Compute all five digests of `path` in a single read pass.
///
/// `progress(done, total)` fires roughly once per 64 MiB so the UI can show a
/// percentage. `on_hash(kind, hex)` fires once per digest, in completion order,
/// the moment that hash's worker finishes, so the UI can swap a per-hash
/// spinner for its value as each lands instead of waiting for all five.
///
/// Each digest runs on its own worker thread, so the wall-clock cost is the
/// slowest single hash rather than the sum of all five. The reader hands every
/// chunk to all five workers as a shared `Arc<[u8]>` (one copy per chunk, then
/// cheap refcount clones); bounded channels apply backpressure so the reader
/// cannot outrun the slowest hash and balloon memory. Call this off the UI
/// thread.
pub fn compute_hashes(
    path: &Path,
    mut progress: impl FnMut(u64, u64),
    mut on_hash: impl FnMut(HashKind, String),
) -> IsoHashes {
    use md5::Md5;
    use sha1::Sha1;
    use sha2::{Digest, Sha256, Sha512};
    use std::sync::Arc;
    use std::sync::mpsc::{channel, sync_channel};

    let Ok(mut file) = File::open(path) else {
        return IsoHashes::default();
    };
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);

    type Chunk = Arc<[u8]>;
    // Bounded so a slow hasher throttles the reader instead of letting unhashed
    // chunks pile up. Depth 4 * 5 workers * 4 MiB caps the backlog at ~80 MiB.
    let (md5_tx, md5_rx) = sync_channel::<Chunk>(4);
    let (sha1_tx, sha1_rx) = sync_channel::<Chunk>(4);
    let (sha256_tx, sha256_rx) = sync_channel::<Chunk>(4);
    let (sha512_tx, sha512_rx) = sync_channel::<Chunk>(4);
    let (blake3_tx, blake3_rx) = sync_channel::<Chunk>(4);
    // Workers report `(kind, hex)` here the instant they finish.
    let (res_tx, res_rx) = channel::<(HashKind, String)>();

    std::thread::scope(|s| {
        let r = res_tx.clone();
        s.spawn(move || {
            let mut h = Md5::new();
            for c in md5_rx {
                h.update(&c[..]);
            }
            let _ = r.send((HashKind::Md5, hex(&h.finalize())));
        });
        let r = res_tx.clone();
        s.spawn(move || {
            let mut h = Sha1::new();
            for c in sha1_rx {
                h.update(&c[..]);
            }
            let _ = r.send((HashKind::Sha1, hex(&h.finalize())));
        });
        let r = res_tx.clone();
        s.spawn(move || {
            let mut h = Sha256::new();
            for c in sha256_rx {
                h.update(&c[..]);
            }
            let _ = r.send((HashKind::Sha256, hex(&h.finalize())));
        });
        let r = res_tx.clone();
        s.spawn(move || {
            let mut h = Sha512::new();
            for c in sha512_rx {
                h.update(&c[..]);
            }
            let _ = r.send((HashKind::Sha512, hex(&h.finalize())));
        });
        let r = res_tx.clone();
        s.spawn(move || {
            let mut h = blake3::Hasher::new();
            for c in blake3_rx {
                h.update(&c[..]);
            }
            let _ = r.send((HashKind::Blake3, h.finalize().to_hex().to_string()));
        });
        // Drop our own handle so `res_rx` closes once the five workers do.
        drop(res_tx);

        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let mut done = 0u64;
        const REPORT_EVERY: u64 = 64 * 1024 * 1024;
        let mut next_report = 0u64;
        let mut read_ok = true;
        loop {
            match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk: Chunk = Arc::from(&buf[..n]);
                    if md5_tx.send(chunk.clone()).is_err()
                        || sha1_tx.send(chunk.clone()).is_err()
                        || sha256_tx.send(chunk.clone()).is_err()
                        || sha512_tx.send(chunk.clone()).is_err()
                        || blake3_tx.send(chunk).is_err()
                    {
                        read_ok = false;
                        break;
                    }
                    done += n as u64;
                    if done >= next_report {
                        progress(done, total);
                        next_report = done + REPORT_EVERY;
                    }
                }
                Err(_) => {
                    read_ok = false;
                    break;
                }
            }
        }
        // Close every input channel so the workers see EOF and finish.
        drop((md5_tx, sha1_tx, sha256_tx, sha512_tx, blake3_tx));

        if !read_ok {
            return IsoHashes::default();
        }
        progress(total.max(done), total.max(done));

        // Collect the five digests in completion order, surfacing each to the
        // caller the moment it arrives so a per-hash spinner can resolve.
        let mut out = IsoHashes::default();
        for (kind, digest) in res_rx {
            match kind {
                HashKind::Md5 => out.md5 = digest.clone(),
                HashKind::Sha1 => out.sha1 = digest.clone(),
                HashKind::Sha256 => out.sha256 = digest.clone(),
                HashKind::Sha512 => out.sha512 = digest.clone(),
                HashKind::Blake3 => out.blake3 = digest.clone(),
            }
            on_hash(kind, digest);
        }
        out
    })
}

/// Lowercase hex encoding of a fixed-size digest.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
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

/// Detect a UDF filesystem by scanning the ISO Volume Recognition Sequence:
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

    #[test]
    fn parses_windows_build_from_wim_xml() {
        let xml = r#"<WIM><TOTALBYTES>1</TOTALBYTES>
            <IMAGE INDEX="1"><NAME>Windows 11 Pro</NAME>
              <WINDOWS><VERSION><MAJOR>10</MAJOR><MINOR>0</MINOR><BUILD>26100</BUILD></VERSION></WINDOWS>
            </IMAGE></WIM>"#;
        assert_eq!(parse_windows_build(xml), 26100);
        assert_eq!(parse_windows_build("<WIM></WIM>"), 0);
    }

    #[test]
    fn parses_windows_arch_from_wim_xml() {
        let xml = r#"<WIM><IMAGE INDEX="1"><WINDOWS><ARCH>9</ARCH></WINDOWS></IMAGE></WIM>"#;
        assert_eq!(parse_windows_arch(xml).as_deref(), Some("amd64"));
        assert_eq!(
            parse_windows_arch("<WINDOWS><ARCH>12</ARCH></WINDOWS>").as_deref(),
            Some("arm64")
        );
        assert_eq!(parse_windows_arch("<ARCH>0</ARCH>").as_deref(), Some("x86"));
        assert_eq!(parse_windows_arch("<ARCH>99</ARCH>"), None); // unknown
        assert_eq!(parse_windows_arch("<WIM></WIM>"), None);
    }

    #[test]
    fn decodes_utf16le_with_bom() {
        // BOM + "Hi"
        let bytes = [0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        assert_eq!(decode_utf16le(&bytes), "Hi");
    }

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

    #[test]
    fn parallel_hashes_match_single_threaded_over_multiple_chunks() {
        use md5::Md5;
        use sha1::Sha1;
        use sha2::{Digest, Sha256, Sha512};

        // ~10 MiB so the reader emits several 4 MiB chunks and the per-worker
        // channels must preserve byte order across threads.
        let data: Vec<u8> = (0..10u32 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let dir = tempdir::Holder::new();
        let path = dir.path().join("hashme.bin");
        std::fs::write(&path, &data).unwrap();

        let mut reported = std::collections::HashSet::new();
        let got = compute_hashes(
            &path,
            |_, _| {},
            |kind, _| {
                reported.insert(kind);
            },
        );
        let want = IsoHashes {
            md5: hex(&Md5::digest(&data)),
            sha1: hex(&Sha1::digest(&data)),
            sha256: hex(&Sha256::digest(&data)),
            sha512: hex(&Sha512::digest(&data)),
            blake3: blake3::hash(&data).to_hex().to_string(),
        };
        assert_eq!(got, want);
        assert_eq!(reported.len(), 5, "on_hash must fire once per digest");
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
