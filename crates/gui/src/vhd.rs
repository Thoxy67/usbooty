//! Recognise and unwrap Microsoft VHD (Virtual Hard Disk v1) images.
//!
//! A *fixed* VHD is byte-for-byte raw disk data followed by a 512-byte footer.
//! Stripping the footer turns the file back into a plain `.img` the rest of
//! usbooty already knows how to write. Dynamic and differencing VHDs use a
//! sparse block-allocation table and are out of scope for this round; users
//! can pre-convert with `qemu-img convert -O raw`.
//!
//! Reference: Microsoft's `Virtual Hard Disk Image Format Specification`,
//! section "Hard Disk Footer Format".

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Magic cookie at offset 0 of the VHD footer.
const COOKIE: &[u8; 8] = b"conectix";

/// VHD footer size in bytes.
const FOOTER_LEN: u64 = 512;

/// VHD `Disk Type` values (footer offset 60, big-endian u32).
const DISK_TYPE_FIXED: u32 = 2;
const DISK_TYPE_DYNAMIC: u32 = 3;
const DISK_TYPE_DIFFERENCING: u32 = 4;

/// What the footer says about this VHD file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VhdKind {
    /// Plain raw data + 512-byte trailer; usable after stripping the footer.
    Fixed {
        /// Bytes of payload before the footer (i.e. file size − 512).
        data_size: u64,
    },
    /// Sparse format, not supported without a full BAT walker.
    DynamicOrDifferencing,
    /// File doesn't look like a VHD at all.
    NotVhd,
}

/// Read the trailing 512 bytes of `path` and decide which VHD flavour, if any,
/// it represents. Errors only when the file can't be opened; an unrecognised
/// footer just returns [`VhdKind::NotVhd`].
pub fn inspect(path: &Path) -> Result<VhdKind> {
    let meta =
        fs::metadata(path).with_context(|| format!("reading metadata of {}", path.display()))?;
    if meta.len() < FOOTER_LEN {
        return Ok(VhdKind::NotVhd);
    }

    let mut footer = [0u8; FOOTER_LEN as usize];
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    f.seek(SeekFrom::End(-(FOOTER_LEN as i64)))
        .context("seeking to VHD footer")?;
    f.read_exact(&mut footer).context("reading VHD footer")?;

    if &footer[0..8] != COOKIE {
        return Ok(VhdKind::NotVhd);
    }

    // Disk Type is at offset 60, 4 bytes, big-endian.
    let disk_type = u32::from_be_bytes([footer[60], footer[61], footer[62], footer[63]]);
    match disk_type {
        DISK_TYPE_FIXED => Ok(VhdKind::Fixed {
            data_size: meta.len() - FOOTER_LEN,
        }),
        DISK_TYPE_DYNAMIC | DISK_TYPE_DIFFERENCING => Ok(VhdKind::DynamicOrDifferencing),
        _ => Ok(VhdKind::NotVhd),
    }
}

/// Materialise a fixed-VHD's payload as a raw `.img` under
/// `~/.cache/usbooty/decompressed/`. The cached file is byte-for-byte the
/// disk image, so it drops straight into the existing analyse/write pipeline.
///
/// Cache key follows the same shape as [`crate::decompress`]: a stable hash
/// of the source's canonical path / size / mtime so repeated picks are free.
pub fn strip_footer_to_cache(src: &Path) -> Result<PathBuf> {
    let kind = inspect(src)?;
    let data_size = match kind {
        VhdKind::Fixed { data_size } => data_size,
        VhdKind::DynamicOrDifferencing => bail!(
            "dynamic/differencing VHDs aren't supported yet; convert to a fixed VHD with \
             `qemu-img convert -O vpc -o subformat=fixed in.vhd out.vhd` (or `-O raw` for img)"
        ),
        VhdKind::NotVhd => bail!("{} doesn't look like a VHD file", src.display()),
    };

    let meta = fs::metadata(src)?;
    let source_mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let dir = cache_dir()?;
    let key = key_for(src, meta.len(), source_mtime);
    let out_path = dir.join(format!("{key}.img"));
    if out_path.is_file()
        && let Ok(m) = fs::metadata(&out_path)
        && m.len() == data_size
    {
        let _ = fs::OpenOptions::new().write(true).open(&out_path);
        return Ok(out_path);
    }

    // Stream the payload into a sibling tempfile, then atomically rename.
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)
        .with_context(|| format!("creating tempfile in {}", dir.display()))?;
    let mut input = File::open(src)?;
    let mut remaining = data_size;
    let mut buf = vec![0u8; 1024 * 1024];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        input
            .read_exact(&mut buf[..want])
            .context("reading VHD payload")?;
        tmp.as_file_mut()
            .write_all(&buf[..want])
            .context("writing VHD payload")?;
        remaining -= want as u64;
    }
    tmp.as_file_mut().sync_all().ok();
    tmp.persist(&out_path)
        .with_context(|| format!("renaming to {}", out_path.display()))?;
    Ok(out_path)
}

/// `~/.cache/usbooty/decompressed/`, shared with the decompressor's cache,
/// since both modules produce ready-to-use `.img` files and a single TTL +
/// prune policy covers them all.
fn cache_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("org", "usbooty", "usbooty")
        .context("cannot determine the user cache directory")?;
    let dir = dirs.cache_dir().join("decompressed");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// 16-hex-char fingerprint of (canonical-path, size, mtime, "vhd"). Same
/// layout the decompressor uses, with a different algorithm tag so the two
/// caches never collide on the same source.
fn key_for(path: &Path, size: u64, mtime: u64) -> String {
    use sha2::{Digest, Sha256};
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut h = Sha256::new();
    h.update(canonical.to_string_lossy().as_bytes());
    h.update(b"\0");
    h.update(size.to_le_bytes());
    h.update(mtime.to_le_bytes());
    h.update(b"vhd");
    let d = h.finalize();
    let mut out = String::with_capacity(32);
    for byte in &d[..16] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
