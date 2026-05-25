//! VHD (Virtual Hard Disk) reader.
//!
//! Supports the two flavors of Microsoft's VHD format that matter in practice:
//!
//! * **Fixed**: the file is a verbatim raw disk image with a 512-byte footer
//!   tacked on the end. The footer carries the virtual disk size and a sanity
//!   cookie. We simply expose the raw payload (everything before the footer).
//!
//! * **Dynamic**: a header + dynamic header + Block Allocation Table (BAT)
//!   describes where each fixed-size block of the virtual disk lives in the
//!   file. Blocks not yet written are absent and read as zero. We translate
//!   virtual offsets to file offsets through the BAT, returning zeros for
//!   unallocated blocks.
//!
//! Differencing VHDs (`DiskType = 4`) need a parent image to resolve, which we
//! cannot do; they are refused with a clear error pointing at `qemu-img`.
//!
//! The format spec ("Virtual Hard Disk Image Format Specification") is short
//! and stable; the relevant constants are all little-endian on disk except for
//! the BAT entries, which are big-endian sector numbers.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

/// VHD footer cookie ("conectix"), present in both fixed and dynamic.
const FOOTER_COOKIE: &[u8; 8] = b"conectix";
/// VHD dynamic-header cookie ("cxsparse").
const DYNAMIC_COOKIE: &[u8; 8] = b"cxsparse";

/// A `Read` source backed by a VHD file, exposing the *virtual* disk contents.
pub struct VhdReader {
    file: File,
    kind: Kind,
    /// Virtual disk size in bytes (what a guest OS sees).
    virtual_size: u64,
    /// Current virtual position. `Read` advances this.
    pos: u64,
}

enum Kind {
    /// Payload occupies bytes `[0, payload_size)`; the rest is footer.
    Fixed { payload_size: u64 },
    /// Block-by-block lookup through the BAT.
    Dynamic {
        block_size: u32,
        /// File offset (in bytes) of each block's data area, with the bitmap
        /// already skipped past. `None` for unallocated blocks (read as zero).
        block_offsets: Vec<Option<u64>>,
    },
}

impl VhdReader {
    /// Open `file` as a VHD. Fails if it is not a recognised VHD, or is a
    /// differencing VHD (which we cannot resolve without the parent image).
    pub fn new(mut file: File) -> Result<Self> {
        let file_size = file.metadata().context("stat VHD")?.len();
        if file_size < 512 {
            bail!("file is too short to be a VHD");
        }
        file.seek(SeekFrom::Start(file_size - 512))
            .context("seeking VHD footer")?;
        let mut footer = [0u8; 512];
        file.read_exact(&mut footer).context("reading VHD footer")?;
        if &footer[0..8] != FOOTER_COOKIE {
            bail!("not a VHD (no `conectix` footer)");
        }

        // Footer fields (all big-endian per the spec):
        //   offset 48: CurrentSize  — virtual disk size in bytes
        //   offset 60: DiskType     — 2=fixed, 3=dynamic, 4=differencing
        //   offset 16: DataOffset   — file offset of the dynamic header
        let virtual_size = u64::from_be_bytes(footer[48..56].try_into().unwrap());
        let disk_type = u32::from_be_bytes(footer[60..64].try_into().unwrap());
        let data_offset = u64::from_be_bytes(footer[16..24].try_into().unwrap());

        let kind = match disk_type {
            2 => Kind::Fixed {
                payload_size: file_size - 512,
            },
            3 => parse_dynamic(&mut file, data_offset, virtual_size)?,
            4 => bail!(
                "differencing VHDs need their parent image and cannot be written directly; \
                 convert to a flat image first, for example with: \
                 qemu-img convert -O raw input.vhd output.img"
            ),
            other => bail!("unsupported VHD disk type {other}"),
        };

        Ok(Self {
            file,
            kind,
            virtual_size,
            pos: 0,
        })
    }

    /// The virtual disk size (in bytes) — i.e., the size of the output stream.
    pub fn virtual_size(&self) -> u64 {
        self.virtual_size
    }
}

/// Parse the dynamic header + BAT, producing a `Kind::Dynamic`.
fn parse_dynamic(file: &mut File, data_offset: u64, virtual_size: u64) -> Result<Kind> {
    file.seek(SeekFrom::Start(data_offset))
        .context("seeking VHD dynamic header")?;
    let mut hdr = [0u8; 1024];
    file.read_exact(&mut hdr)
        .context("reading VHD dynamic header")?;
    if &hdr[0..8] != DYNAMIC_COOKIE {
        bail!("VHD dynamic header is missing the `cxsparse` cookie");
    }

    // Dynamic-header fields (big-endian):
    //   offset 16: TableOffset      — file offset of the BAT (8 bytes)
    //   offset 28: MaxTableEntries  — count of BAT entries (4 bytes)
    //   offset 32: BlockSize        — block size in bytes (4 bytes, typ. 2 MiB)
    let table_offset = u64::from_be_bytes(hdr[16..24].try_into().unwrap());
    let max_entries = u32::from_be_bytes(hdr[28..32].try_into().unwrap());
    let block_size = u32::from_be_bytes(hdr[32..36].try_into().unwrap());
    if block_size == 0 || !block_size.is_power_of_two() {
        bail!("VHD dynamic header has invalid block size {block_size}");
    }

    // Bytes for the per-block sector bitmap, padded up to 512.
    let sectors_per_block = block_size / 512;
    let bitmap_bytes = ((sectors_per_block + 7) / 8).max(1);
    let bitmap_bytes = ((bitmap_bytes + 511) / 512) * 512;

    // Read the BAT itself: each entry is a 4-byte big-endian sector number
    // pointing at the block's bitmap + data area, or 0xFFFFFFFF if absent.
    file.seek(SeekFrom::Start(table_offset))
        .context("seeking BAT")?;
    let mut raw = vec![0u8; max_entries as usize * 4];
    file.read_exact(&mut raw).context("reading BAT")?;
    let mut block_offsets = Vec::with_capacity(max_entries as usize);
    for chunk in raw.chunks_exact(4) {
        let entry = u32::from_be_bytes(chunk.try_into().unwrap());
        if entry == 0xFFFF_FFFF {
            block_offsets.push(None);
        } else {
            // Sector number → byte offset of the bitmap; data starts after.
            let byte_off = entry as u64 * 512 + bitmap_bytes as u64;
            block_offsets.push(Some(byte_off));
        }
    }

    if (max_entries as u64) * (block_size as u64) < virtual_size {
        bail!("VHD BAT cannot cover the declared virtual size");
    }

    let _ = bitmap_bytes; // already folded into `block_offsets` above
    Ok(Kind::Dynamic {
        block_size,
        block_offsets,
    })
}

impl Read for VhdReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.virtual_size {
            return Ok(0);
        }
        let want = buf.len().min((self.virtual_size - self.pos) as usize);
        if want == 0 {
            return Ok(0);
        }
        let dst = &mut buf[..want];

        let n = match &self.kind {
            Kind::Fixed { payload_size } => {
                // Bounds already enforced by `want`; payload_size == virtual_size.
                let _ = payload_size;
                self.file.seek(SeekFrom::Start(self.pos))?;
                self.file.read(dst)?
            }
            Kind::Dynamic {
                block_size,
                block_offsets,
            } => {
                let block_size = *block_size as u64;
                let block_idx = (self.pos / block_size) as usize;
                let offset_in_block = self.pos % block_size;
                let take = ((block_size - offset_in_block) as usize).min(dst.len());
                match block_offsets.get(block_idx).copied().flatten() {
                    Some(file_off) => {
                        self.file
                            .seek(SeekFrom::Start(file_off + offset_in_block))?;
                        self.file.read(&mut dst[..take])?
                    }
                    None => {
                        // Sparse block: emit zeros.
                        for byte in dst[..take].iter_mut() {
                            *byte = 0;
                        }
                        take
                    }
                }
            }
        };
        self.pos += n as u64;
        Ok(n)
    }
}

/// Refuse a VHDX with actionable guidance. Full VHDX support (header pages,
/// region table, BAT, log replay) is substantial and not yet implemented; the
/// recommended path is to flatten with `qemu-img` first.
pub fn refuse_vhdx() -> anyhow::Error {
    anyhow::anyhow!(
        ".vhdx images are not yet supported directly; convert to a flat image first, \
         for example with: qemu-img convert -O raw input.vhdx output.img"
    )
}
