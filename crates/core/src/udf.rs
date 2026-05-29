//! Minimal read-only UDF (Universal Disk Format) reader.
//!
//! Modern Windows installer ISOs are primarily UDF: the ISO 9660 stub they
//! also carry is near-empty, so a userspace ISO9660 reader (`cdfs`) sees
//! almost nothing. This parser walks a UDF 1.02 / 2.01 image just far enough
//! to list directory contents and read small files: enough for usbooty's
//! analysis pass (oversized `install.wim` detection, SBAT scan of
//! `/EFI/BOOT/*.efi`, label, etc.) without mounting the ISO or running as
//! root.
//!
//! Coverage scope (deliberately narrow):
//!
//! * UDF block size 2048 (the only value used on optical / install media).
//! * Anchor Volume Descriptor Pointer at sector 256 only — the spec also
//!   permits last-sector and last-256, but every UDF-formatted Windows ISO
//!   in the wild carries the canonical anchor.
//! * Short-AD and long-AD allocation descriptors; no allocation extents
//!   (indirect descriptors), no metadata partitions, no virtual partitions.
//! * Multi-extent files (so install.wim larger than 4 GiB is read correctly).
//! * No CRC verification — kernel `mount -t udf` already failed by then if
//!   the disc is corrupt; we just want best-effort analysis here.
//!
//! All fields on disk are little-endian.

use std::io::{Read, Result as IoResult, Seek, SeekFrom};

/// UDF block size for optical and install media.
pub const BLOCK: u64 = 2048;

/// Tag identifiers we recognise (ECMA-167 / OSTA UDF).
const TAG_PVD: u16 = 1;
const TAG_AVDP: u16 = 2;
const TAG_PARTITION: u16 = 5;
const TAG_LVD: u16 = 6;
const TAG_TERMINATING: u16 = 8;
const TAG_FSD: u16 = 256;
const TAG_FID: u16 = 257;
const TAG_FILE_ENTRY: u16 = 261;
const TAG_EXTENDED_FILE_ENTRY: u16 = 266;

/// `FileCharacteristics` bits inside a File Identifier Descriptor.
const FID_DIRECTORY: u8 = 0x02;
const FID_DELETED: u8 = 0x04;
const FID_PARENT: u8 = 0x08;

/// Top-level UDF filesystem handle.
pub struct UdfFs<R: Read + Seek> {
    reader: R,
    /// Where the first (typically only) UDF partition begins, in 2048-byte
    /// sectors absolute to the start of the image.
    partition_start: u32,
    /// Location of the root directory's ICB (an extent descriptor relative
    /// to `partition_start`).
    root_icb: ExtentRef,
    /// Volume label drawn from the Primary Volume Descriptor's volume ID.
    label: String,
}

/// A (block-location, length-in-bytes) reference to an extent inside the
/// partition. UDF stores both short_ad (no partition number) and long_ad
/// (with partition number); we collapse them to this form.
#[derive(Debug, Clone, Copy)]
pub struct ExtentRef {
    pub block: u32,
    pub length: u32,
}

/// A single entry inside a UDF directory.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub icb: ExtentRef,
}

impl<R: Read + Seek> UdfFs<R> {
    /// Parse the volume descriptors and locate the root directory.
    /// Returns `None` if the image is not recognisable UDF, never panics.
    pub fn open(mut reader: R) -> Option<Self> {
        // Anchor Volume Descriptor Pointer (AVDP) at sector 256.
        let avdp = read_sector(&mut reader, 256).ok()?;
        if tag_id(&avdp)? != TAG_AVDP {
            return None;
        }
        let main_vds_len = u32::from_le_bytes(avdp[16..20].try_into().ok()?);
        let main_vds_loc = u32::from_le_bytes(avdp[20..24].try_into().ok()?);

        // Walk the Main Volume Descriptor Sequence collecting the Partition
        // Descriptor (where data lives) and the LVD (where the root is).
        let mut partition_start = None;
        let mut label = String::new();
        let mut fsd_extent: Option<ExtentRef> = None;
        let sectors = (main_vds_len as u64).div_ceil(BLOCK);
        for i in 0..sectors {
            let buf = read_sector(&mut reader, main_vds_loc as u64 + i).ok()?;
            match tag_id(&buf) {
                Some(TAG_PVD) => label = read_dstring(&buf[24..56]),
                Some(TAG_PARTITION) => {
                    // Partition Descriptor (ECMA-167 3/10.5):
                    //   +188: u32 partition_starting_location
                    partition_start = Some(u32::from_le_bytes(buf[188..192].try_into().ok()?));
                }
                Some(TAG_LVD) => {
                    // Logical Volume Descriptor (ECMA-167 3/10.6) — the File
                    // Set Descriptor location is a long_ad at offset 248.
                    fsd_extent = Some(read_long_ad(&buf[248..264])?);
                }
                Some(TAG_TERMINATING) => break,
                _ => {}
            }
        }
        let partition_start = partition_start?;
        let fsd_ref = fsd_extent?;

        // The FSD lives inside the partition; its long_ad location is a
        // block index inside the partition, not an absolute sector.
        let fsd = read_sector(&mut reader, partition_start as u64 + fsd_ref.block as u64).ok()?;
        if tag_id(&fsd)? != TAG_FSD {
            return None;
        }
        // FSD (ECMA-167 4/14.1): root_directory_icb is a long_ad at +400.
        let root_icb = read_long_ad(&fsd[400..416])?;

        Some(Self {
            reader,
            partition_start,
            root_icb,
            label,
        })
    }

    /// The volume label decoded from the Primary Volume Descriptor.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Walk into a `/`-segment path and list the children of the resulting
    /// directory (or `None` if the path is not a directory or any segment
    /// is missing). Segment matching is case-insensitive.
    pub fn list(&mut self, segments: &[&str]) -> Option<Vec<DirEntry>> {
        let mut current = self.root_icb;
        for seg in segments {
            let children = self.read_dir(current)?;
            let found = children
                .into_iter()
                .find(|e| e.is_dir && e.name.eq_ignore_ascii_case(seg))?;
            current = found.icb;
        }
        self.read_dir(current)
    }

    /// Read the full contents of the file at the segmented path.
    /// Returns `None` if the file does not exist.
    pub fn read_file(&mut self, segments: &[&str]) -> Option<Vec<u8>> {
        let (name, dir_segs) = segments.split_last()?;
        let parent = self.list(dir_segs)?;
        let entry = parent
            .into_iter()
            .find(|e| !e.is_dir && e.name.eq_ignore_ascii_case(name))?;
        self.read_file_data(entry.icb).ok()
    }

    /// Information length of the file at the segmented path, by reading just
    /// the File Entry header. Cheap compared to [`read_file`], so safe to use
    /// for sizing a multi-gigabyte `install.wim`.
    pub fn file_size(&mut self, segments: &[&str]) -> Option<u64> {
        let (name, dir_segs) = segments.split_last()?;
        let parent = self.list(dir_segs)?;
        let entry = parent
            .into_iter()
            .find(|e| !e.is_dir && e.name.eq_ignore_ascii_case(name))?;
        self.read_info_length(entry.icb)
    }

    /// Read `information_length` from the File Entry / Extended File Entry at
    /// `icb`, without reading the file's data. The field lives at byte 56 in
    /// both layouts (ECMA-167 4/14.9 and 4/14.17).
    fn read_info_length(&mut self, icb: ExtentRef) -> Option<u64> {
        let buf = read_sector(
            &mut self.reader,
            self.partition_start as u64 + icb.block as u64,
        )
        .ok()?;
        match tag_id(&buf)? {
            TAG_FILE_ENTRY | TAG_EXTENDED_FILE_ENTRY => {
                Some(u64::from_le_bytes(buf[56..64].try_into().ok()?))
            }
            _ => None,
        }
    }

    // ---- internals -------------------------------------------------------

    /// Read and parse the directory whose File Entry lives at `icb`.
    fn read_dir(&mut self, icb: ExtentRef) -> Option<Vec<DirEntry>> {
        let data = self.read_file_data(icb).ok()?;
        let mut out = Vec::new();
        let mut p = 0;
        while p + 38 <= data.len() {
            // File Identifier Descriptor: 16-byte tag, then:
            //   +16: u16 version
            //   +18: u8  file_characteristics
            //   +19: u8  length_of_file_identifier (L_FI)
            //   +20: long_ad icb (16 bytes)
            //   +36: u16 length_of_implementation_use (L_IU)
            //   +38: implementation_use[L_IU]
            //   +38+L_IU: identifier[L_FI]
            //   then padded to 4-byte boundary.
            if tag_id(&data[p..]).unwrap_or(0) != TAG_FID {
                break;
            }
            let chars = data[p + 18];
            let l_fi = data[p + 19] as usize;
            let icb_bytes = &data[p + 20..p + 36];
            let l_iu = u16::from_le_bytes(data[p + 36..p + 38].try_into().ok()?) as usize;
            // The lengths come from untrusted image bytes; fold them with
            // checked arithmetic so a hostile descriptor can't wrap the offset
            // past the bounds check below. (On 64-bit these can't actually
            // overflow given p <= data.len() and the u8/u16 fields, but the
            // checks document intent and keep 32-bit targets safe.)
            let Some(fi_start) = p.checked_add(38).and_then(|x| x.checked_add(l_iu)) else {
                break;
            };
            let Some(fi_end) = fi_start.checked_add(l_fi) else {
                break;
            };
            if fi_end > data.len() {
                break;
            }
            // total = fi_end - p, already proven <= data.len(), so the round-up
            // can't overflow. padded is >= 40, so `p` always advances.
            let total = 38 + l_iu + l_fi;
            let padded = (total + 3) & !3;

            // Skip deleted entries and the "parent" pseudo-entry.
            if chars & FID_DELETED == 0 && chars & FID_PARENT == 0 {
                let name = decode_udf_name(&data[fi_start..fi_end]);
                let child_icb = read_long_ad(icb_bytes)?;
                out.push(DirEntry {
                    name,
                    is_dir: chars & FID_DIRECTORY != 0,
                    icb: child_icb,
                });
            }
            p += padded;
        }
        Some(out)
    }

    /// Read the data bytes of a file whose File Entry lives at `icb`.
    fn read_file_data(&mut self, icb: ExtentRef) -> IoResult<Vec<u8>> {
        let buf = read_sector(
            &mut self.reader,
            self.partition_start as u64 + icb.block as u64,
        )?;
        let tag = tag_id(&buf).ok_or_else(|| std::io::Error::other("missing tag header"))?;
        // File Entry (261) and Extended File Entry (266) have different
        // layouts; the bits we need are at different offsets in each.
        let (l_ea, l_ad, ad_type, ads_start, information_length) = match tag {
            TAG_FILE_ENTRY => {
                // ECMA-167 4/14.9 File Entry layout (byte offsets):
                //   +0..16   descriptor tag
                //   +16..36  ICB tag (20 bytes)
                //   +56      u64 information_length
                //   +160     u64 unique_id
                //   +168     u32 length_of_extended_attributes (L_EA)
                //   +172     u32 length_of_allocation_descriptors (L_AD)
                //   +176     extended_attributes[L_EA], then allocation_descriptors[L_AD]
                // ICB tag's flags at offset +16+18 (= +34) carry the AD type
                // (low 3 bits): 0=short_ad, 1=long_ad, 3=embedded.
                let ad_type = u16::from_le_bytes(buf[34..36].try_into().unwrap()) & 0x7;
                let info_len = u64::from_le_bytes(buf[56..64].try_into().unwrap());
                let l_ea = u32::from_le_bytes(buf[168..172].try_into().unwrap()) as usize;
                let l_ad = u32::from_le_bytes(buf[172..176].try_into().unwrap()) as usize;
                (l_ea, l_ad, ad_type, 176, info_len)
            }
            TAG_EXTENDED_FILE_ENTRY => {
                // Extended File Entry (ECMA-167 4/14.17): same idea, larger
                // fixed prefix. ICB tag flags at +34.
                //   +56: u64 information_length
                //   +208: u32 L_EA
                //   +212: u32 L_AD
                //   +216: extended_attributes[L_EA] then allocation_descriptors
                let ad_type = u16::from_le_bytes(buf[34..36].try_into().unwrap()) & 0x7;
                let info_len = u64::from_le_bytes(buf[56..64].try_into().unwrap());
                let l_ea = u32::from_le_bytes(buf[208..212].try_into().unwrap()) as usize;
                let l_ad = u32::from_le_bytes(buf[212..216].try_into().unwrap()) as usize;
                (l_ea, l_ad, ad_type, 216, info_len)
            }
            _ => return Err(std::io::Error::other("not a file entry")),
        };

        let ad_offset = ads_start + l_ea;
        let ad_end = ad_offset + l_ad;
        if ad_end > buf.len() {
            return Err(std::io::Error::other("allocation descriptors out of range"));
        }

        let mut out = Vec::with_capacity(information_length.min(64 * 1024 * 1024) as usize);
        match ad_type {
            3 => {
                // Inline / embedded data: the file's bytes live inline where
                // the allocation descriptors would be. Common for very small
                // files (boot configs etc.).
                let take = (information_length as usize).min(l_ad);
                out.extend_from_slice(&buf[ad_offset..ad_offset + take]);
            }
            0 => {
                // Short_ad entries: each is 8 bytes: u32 length, u32 position.
                self.read_extents(&buf[ad_offset..ad_end], 8, information_length, &mut out)?;
            }
            1 => {
                // Long_ad entries: each is 16 bytes (length, position, then a
                // partition reference + implementation-use field we ignore).
                self.read_extents(&buf[ad_offset..ad_end], 16, information_length, &mut out)?;
            }
            other => {
                return Err(std::io::Error::other(format!(
                    "unsupported allocation type {other}"
                )));
            }
        }
        Ok(out)
    }

    /// Walk a buffer of allocation descriptors with the given `stride`
    /// (8 bytes per short_ad, 16 bytes per long_ad — the trailing partition
    /// reference and implementation-use bytes of a long_ad are not used by
    /// this reader, so the parse only cares about the leading length + block
    /// position fields, which match in both layouts).
    fn read_extents(
        &mut self,
        descriptors: &[u8],
        stride: usize,
        info_len: u64,
        out: &mut Vec<u8>,
    ) -> IoResult<()> {
        let mut remaining = info_len as usize;
        for ad in descriptors.chunks_exact(stride) {
            if remaining == 0 {
                break;
            }
            let raw_len = u32::from_le_bytes(ad[0..4].try_into().unwrap());
            let pos = u32::from_le_bytes(ad[4..8].try_into().unwrap());
            // Top two bits: 0=recorded, 1=unrecorded but allocated (sparse),
            // 2=not allocated, 3=extent of allocation descriptors (chained).
            let kind = raw_len >> 30;
            let length = (raw_len & 0x3FFF_FFFF) as usize;
            if length == 0 {
                break;
            }
            let take = length.min(remaining);
            if kind == 0 {
                self.read_extent_into(self.partition_start as u64 + pos as u64, take, out)?;
            } else if kind == 1 || kind == 2 {
                // Sparse: emit zeros.
                out.resize(out.len() + take, 0);
            } else {
                break; // unsupported AD-chain
            }
            remaining -= take;
        }
        Ok(())
    }

    /// Read `len` bytes starting at sector `block` into `out`.
    fn read_extent_into(&mut self, block: u64, len: usize, out: &mut Vec<u8>) -> IoResult<()> {
        self.reader.seek(SeekFrom::Start(block * BLOCK))?;
        let start = out.len();
        out.resize(start + len, 0);
        self.reader.read_exact(&mut out[start..start + len])?;
        Ok(())
    }
}

// ---- byte-level helpers ---------------------------------------------------

fn read_sector<R: Read + Seek>(reader: &mut R, sector: u64) -> IoResult<Vec<u8>> {
    reader.seek(SeekFrom::Start(sector * BLOCK))?;
    let mut buf = vec![0u8; BLOCK as usize];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

/// Tag identifier at the start of a UDF descriptor (always at byte 0..2 LE).
fn tag_id(buf: &[u8]) -> Option<u16> {
    if buf.len() < 16 {
        return None;
    }
    Some(u16::from_le_bytes(buf[0..2].try_into().ok()?))
}

/// Parse a 16-byte long_ad: u32 length, u32 logical block, u16 partition
/// reference, 6-byte implementation use. We collapse to our `ExtentRef`.
fn read_long_ad(buf: &[u8]) -> Option<ExtentRef> {
    if buf.len() < 16 {
        return None;
    }
    let length = u32::from_le_bytes(buf[0..4].try_into().ok()?);
    let block = u32::from_le_bytes(buf[4..8].try_into().ok()?);
    Some(ExtentRef { block, length })
}

/// Decode a UDF "d-string" (used for the volume label): a single-byte
/// compression identifier (8 = 8-bit OSTA, 16 = 16-bit UCS-2BE), the
/// payload, and a trailing length byte at the end of the field. The trailing
/// length is the count of *payload* bytes including the compression byte.
fn read_dstring(buf: &[u8]) -> String {
    if buf.len() < 2 {
        return String::new();
    }
    let total = *buf.last().unwrap() as usize;
    if total == 0 || total > buf.len() - 1 {
        return String::new();
    }
    let payload = &buf[1..total];
    let comp = buf[0];
    match comp {
        8 => String::from_utf8_lossy(payload).trim().to_string(),
        16 => {
            // chunks(2) (not chunks_exact) so a malformed odd-length payload
            // doesn't silently lose its final byte — the lone trailing byte
            // is just ignored here, same end-result for well-formed data.
            let mut s = String::with_capacity(payload.len() / 2);
            for pair in payload.chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let c = u16::from_be_bytes([pair[0], pair[1]]);
                if let Some(ch) = char::from_u32(c as u32) {
                    s.push(ch);
                }
            }
            s.trim().to_string()
        }
        _ => String::new(),
    }
}

/// Decode a UDF File Identifier name. Same compression-byte rule as a
/// d-string, but the *first* byte of the identifier carries the marker and
/// the rest is the raw payload (no trailing length byte).
fn decode_udf_name(buf: &[u8]) -> String {
    if buf.is_empty() {
        return String::new();
    }
    match buf[0] {
        8 => String::from_utf8_lossy(&buf[1..]).to_string(),
        16 => {
            // chunks(2) (not chunks_exact): same rationale as read_dstring.
            let mut s = String::with_capacity(buf.len() / 2);
            for pair in buf[1..].chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let c = u16::from_be_bytes([pair[0], pair[1]]);
                if let Some(ch) = char::from_u32(c as u32) {
                    s.push(ch);
                }
            }
            s
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_8_bit_dstring() {
        // "HELLO" in OSTA 8-bit: marker 8, payload "HELLO", trailing len = 6.
        let buf = [8, b'H', b'E', b'L', b'L', b'O', 0, 0, 6];
        assert_eq!(read_dstring(&buf), "HELLO");
    }

    #[test]
    fn decodes_16_bit_dstring() {
        // "OK" in UCS-2BE: marker 16, 0,'O', 0,'K', trailing len = 5.
        let buf = [16, 0, b'O', 0, b'K', 0, 0, 0, 0, 5];
        assert_eq!(read_dstring(&buf), "OK");
    }

    #[test]
    fn decodes_8_bit_filename() {
        assert_eq!(
            decode_udf_name(&[8, b'i', b'n', b's', b't', b'a', b'l', b'l']),
            "install"
        );
    }
}
