//! UEFI Secure Boot revocation data: SBAT generation levels and DBX hashes.
//!
//! When a Secure-Boot-signed bootloader (shim, grub, kernel) is found to be
//! exploitable, UEFI firmware can refuse to load future copies of it via two
//! mechanisms:
//!
//! * **SBAT** — every signed binary embeds a `.sbat` CSV section listing the
//!   product name and a generation number. The firmware's stored
//!   "SbatLevel" variable rejects anything whose generation is lower than the
//!   level for its product. This is what is normally tripped today.
//! * **DBX** — a list of forbidden Authenticode hashes; matching binaries are
//!   refused outright. Used for one-off revocations and pre-SBAT vulnerable
//!   shims that cannot be retroactively re-signed.
//!
//! We ship a baked-in fallback so warnings work offline; an optional cache
//! refresh (handled by the GUI) can replace it with the live data Microsoft
//! publishes. This module is dependency-light on purpose — it parses the
//! formats with primitive ops only, no PE parser pulled in.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

/// Required minimum generation numbers, keyed by SBAT product name.
/// A binary whose product generation is *lower* than its product's level here
/// would be refused by SBAT-aware firmware.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SbatLevel {
    pub products: HashMap<String, u32>,
}

/// Set of forbidden Authenticode SHA-256 hashes (each 32 raw bytes).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DbxHashes(pub HashSet<[u8; 32]>);

/// Combined revocation database used to flag binaries in an ISO.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevocationDb {
    pub sbat: SbatLevel,
    pub dbx: DbxHashes,
}

impl RevocationDb {
    /// Built-in baseline. Reflects the publicly-documented minimum SBAT
    /// generation levels enforced by Microsoft as of the most recent advisory
    /// the project bundles. The GUI can swap this for fresher data via the
    /// cache without rebuilding.
    pub fn baked_in() -> Self {
        let products = [
            // Shim project (the first Secure-Boot loader most distros use).
            ("shim", 4u32),
            // GRUB upstream.
            ("grub", 4u32),
            // Linux kernel "EFI stub" signed entries (Ubuntu, Fedora).
            ("linux", 1u32),
            // Vendor-specific grub branches.
            ("grub.debian", 4u32),
            ("grub.ubuntu", 4u32),
            ("grub.fedora", 4u32),
            ("grub.opensuse", 4u32),
            ("grub.suse", 4u32),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        Self {
            sbat: SbatLevel { products },
            dbx: DbxHashes::default(),
        }
    }
}

/// One entry parsed out of a `.sbat` section's CSV body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbatEntry {
    pub component: String,
    pub generation: u32,
}

/// Parse the textual `.sbat` section embedded in a signed EFI binary.
///
/// The section is a CSV: one header row (the SBAT meta), then one row per
/// product. Each row's first column is the component name, the second is the
/// generation number. Anything past the second column (vendor URL, etc.) is
/// ignored. Malformed lines are skipped.
pub fn parse_sbat_section(text: &str) -> Vec<SbatEntry> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // First non-comment line is the meta header `sbat,<gen>,...`; skip.
        if i == 0 && line.starts_with("sbat,") {
            continue;
        }
        let mut cols = line.split(',');
        let Some(component) = cols.next() else {
            continue;
        };
        let Some(gen_str) = cols.next() else {
            continue;
        };
        let Ok(generation) = gen_str.trim().parse::<u32>() else {
            continue;
        };
        out.push(SbatEntry {
            component: component.trim().to_string(),
            generation,
        });
    }
    out
}

/// Return human-readable warnings for any SBAT entry whose generation is
/// below the required minimum recorded in `db`.
pub fn evaluate_sbat(entries: &[SbatEntry], db: &SbatLevel) -> Vec<String> {
    let mut warnings = Vec::new();
    for entry in entries {
        if let Some(&min) = db.products.get(&entry.component) {
            if entry.generation < min {
                warnings.push(format!(
                    "{} SBAT generation {} is below the required {}; \
                     firmware with current revocations will refuse to boot it",
                    entry.component, entry.generation, min
                ));
            }
        }
    }
    warnings
}

/// Locate and decode the `.sbat` section in a PE32+ binary (i.e. a signed EFI
/// loader). Returns `None` when the binary is not a PE, has no `.sbat`
/// section, or the section bounds run off the end of the file. Designed to
/// be infallible against arbitrary input.
pub fn extract_sbat(pe: &[u8]) -> Option<String> {
    // PE layout, in summary (offsets relative to `pe[0]`):
    //   0x00:  "MZ" DOS signature
    //   0x3C:  u32  e_lfanew     → offset of the PE header
    //   PE+0:  "PE\0\0"          (4 bytes)
    //   PE+4:  COFF File Header (20 bytes)
    //          [+16] u16 SizeOfOptionalHeader
    //          [+18] u16 NumberOfSections (we use NoS at offset 2)
    //   PE+24: Optional Header (variable, sized by SizeOfOptionalHeader)
    //   Then:  Section table — 40 bytes per entry.
    if pe.len() < 0x40 || &pe[0..2] != b"MZ" {
        return None;
    }
    let e_lfanew = u32::from_le_bytes(pe.get(0x3C..0x40)?.try_into().ok()?) as usize;
    if pe.len() < e_lfanew + 24 || &pe[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
        return None;
    }
    let coff = e_lfanew + 4;
    let num_sections = u16::from_le_bytes(pe.get(coff + 2..coff + 4)?.try_into().ok()?) as usize;
    let size_of_opt = u16::from_le_bytes(pe.get(coff + 16..coff + 18)?.try_into().ok()?) as usize;
    let section_table = coff + 20 + size_of_opt;

    for i in 0..num_sections {
        let off = section_table + i * 40;
        let entry = pe.get(off..off + 40)?;
        // Section name is 8 bytes, NUL-padded.
        let name_end = entry[..8].iter().position(|&b| b == 0).unwrap_or(8);
        let name = std::str::from_utf8(&entry[..name_end]).ok()?;
        if name != ".sbat" {
            continue;
        }
        let virtual_size = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let raw_size = u32::from_le_bytes(entry[16..20].try_into().ok()?) as usize;
        let raw_ptr = u32::from_le_bytes(entry[20..24].try_into().ok()?) as usize;
        let take = virtual_size.min(raw_size);
        let bytes = pe.get(raw_ptr..raw_ptr + take)?;
        // Strip trailing NULs that pad the section out to file alignment.
        let trimmed_end = bytes
            .iter()
            .rposition(|&b| b != 0)
            .map(|p| p + 1)
            .unwrap_or(0);
        return std::str::from_utf8(&bytes[..trimmed_end])
            .ok()
            .map(str::to_string);
    }
    None
}

/// The `SignatureType` GUID Microsoft uses for SHA-256 binary hashes in
/// DBX entries (`c1c41626-504c-4092-aca9-41f936934328`).
const EFI_CERT_SHA256_GUID: [u8; 16] = [
    0x26, 0x16, 0xc4, 0xc1, 0x4c, 0x50, 0x92, 0x40, 0xac, 0xa9, 0x41, 0xf9, 0x36, 0x93, 0x43, 0x28,
];

/// Parse a UEFI Forum `dbxupdate_<arch>.bin` and return every revoked
/// Authenticode SHA-256 hash it lists. The file is a signed
/// `EFI_VARIABLE_AUTHENTICATION_2` envelope wrapping a sequence of
/// `EFI_SIGNATURE_LIST` records; we skip the envelope and walk the lists.
///
/// Non-SHA-256 entries (x509 certs, SHA-384/512) are ignored — usbooty
/// only matches Authenticode hashes today. Malformed input returns an
/// empty set rather than panicking; the caller is expected to fall back
/// to the baked-in DB.
pub fn parse_dbxupdate(bytes: &[u8]) -> DbxHashes {
    let mut out = DbxHashes::default();
    // EFI_VARIABLE_AUTHENTICATION_2 layout:
    //   [0..16]   EFI_TIME TimeStamp
    //   [16..]    WIN_CERTIFICATE_UEFI_GUID:
    //               [+0]  u32 dwLength  (size of the entire WIN_CERTIFICATE)
    //               [+4]  u16 wRevision
    //               [+6]  u16 wCertificateType
    //               [+8]  GUID CertType  (PKCS7 signature wrapper)
    //               [+24] CertData[dwLength - 24]
    //   then the unsigned payload (signature list array).
    let Some(dw_length_bytes) = bytes.get(16..20) else {
        return out;
    };
    let dw_length = u32::from_le_bytes(dw_length_bytes.try_into().unwrap_or([0; 4])) as usize;
    let payload_start = 16usize.saturating_add(dw_length);
    let mut cursor = match bytes.get(payload_start..) {
        Some(rest) => rest,
        None => return out,
    };

    while cursor.len() >= 28 {
        let sig_type: [u8; 16] = match cursor[0..16].try_into() {
            Ok(g) => g,
            Err(_) => return out,
        };
        let list_size = u32::from_le_bytes(cursor[16..20].try_into().unwrap_or([0; 4])) as usize;
        let header_size = u32::from_le_bytes(cursor[20..24].try_into().unwrap_or([0; 4])) as usize;
        let sig_size = u32::from_le_bytes(cursor[24..28].try_into().unwrap_or([0; 4])) as usize;
        if list_size < 28 || list_size > cursor.len() || sig_size <= 16 {
            return out;
        }
        let body_start = 28usize.saturating_add(header_size);
        let body = match cursor.get(body_start..list_size) {
            Some(b) => b,
            None => return out,
        };

        if sig_type == EFI_CERT_SHA256_GUID && sig_size == 16 + 32 {
            // Each entry: 16-byte SignatureOwner GUID + 32-byte SHA-256.
            for chunk in body.chunks_exact(sig_size) {
                if let Ok(hash) = chunk[16..16 + 32].try_into() {
                    out.0.insert(hash);
                }
            }
        }
        // Skip to the next signature list.
        cursor = &cursor[list_size..];
    }
    out
}

impl RevocationDb {
    /// Merge an extra set of SHA-256 hashes into `self.dbx`. Used by the
    /// GUI after fetching a fresh `dbxupdate_<arch>.bin` so the baked-in
    /// baseline gets augmented without rebuilding the rest of the DB.
    pub fn extend_dbx(&mut self, extra: DbxHashes) {
        self.dbx.0.extend(extra.0);
    }
}

#[cfg(test)]
mod dbx_tests {
    use super::*;

    /// Build a minimal `dbxupdate.bin` consisting of one `EFI_SIGNATURE_LIST`
    /// of SHA-256 hashes and the smallest valid envelope around it.
    fn synth_dbx(hashes: &[[u8; 32]]) -> Vec<u8> {
        let mut out = Vec::new();
        // 16-byte TimeStamp (zeros).
        out.extend_from_slice(&[0u8; 16]);
        // WIN_CERTIFICATE_UEFI_GUID, minimum legal payload (24 bytes header + 0 CertData).
        let dw_length: u32 = 24;
        out.extend_from_slice(&dw_length.to_le_bytes()); // [16..20]
        out.extend_from_slice(&0x0200u16.to_le_bytes()); // wRevision
        out.extend_from_slice(&0x0EF1u16.to_le_bytes()); // wCertificateType
        out.extend_from_slice(&[0u8; 16]); // CertType GUID

        // EFI_SIGNATURE_LIST header.
        out.extend_from_slice(&EFI_CERT_SHA256_GUID);
        let sig_size: u32 = 16 + 32;
        let list_size: u32 = 28 + hashes.len() as u32 * sig_size;
        out.extend_from_slice(&list_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // SignatureHeaderSize
        out.extend_from_slice(&sig_size.to_le_bytes());
        for h in hashes {
            out.extend_from_slice(&[0u8; 16]); // SignatureOwner GUID
            out.extend_from_slice(h);
        }
        out
    }

    #[test]
    fn parses_a_minimal_dbx_update() {
        let h1 = [0x11u8; 32];
        let h2 = [0x22u8; 32];
        let bytes = synth_dbx(&[h1, h2]);
        let dbx = parse_dbxupdate(&bytes);
        assert_eq!(dbx.0.len(), 2);
        assert!(dbx.0.contains(&h1));
        assert!(dbx.0.contains(&h2));
    }

    #[test]
    fn rejects_truncated_input() {
        let dbx = parse_dbxupdate(&[]);
        assert!(dbx.0.is_empty());
        let dbx = parse_dbxupdate(&[0u8; 8]);
        assert!(dbx.0.is_empty());
    }

    #[test]
    fn extend_dbx_merges_into_existing_set() {
        let mut db = RevocationDb::baked_in();
        let mut extra = DbxHashes::default();
        extra.0.insert([0x99; 32]);
        db.extend_dbx(extra);
        assert!(db.dbx.0.contains(&[0x99; 32]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_sbat_section() {
        let text = "sbat,1,SBAT Version,sbat,1\nshim,3,UEFI shim\ngrub,2,Free Software Foundation";
        let entries = parse_sbat_section(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].component, "shim");
        assert_eq!(entries[0].generation, 3);
        assert_eq!(entries[1].component, "grub");
        assert_eq!(entries[1].generation, 2);
    }

    #[test]
    fn flags_old_shim_against_baked_in_level() {
        let entries = vec![SbatEntry {
            component: "shim".to_string(),
            generation: 2,
        }];
        let warnings = evaluate_sbat(&entries, &RevocationDb::baked_in().sbat);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("shim"));
    }

    #[test]
    fn passes_a_fresh_shim() {
        let entries = vec![SbatEntry {
            component: "shim".to_string(),
            generation: 99,
        }];
        let warnings = evaluate_sbat(&entries, &RevocationDb::baked_in().sbat);
        assert!(warnings.is_empty());
    }
}
