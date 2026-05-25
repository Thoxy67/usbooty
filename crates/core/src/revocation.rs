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
