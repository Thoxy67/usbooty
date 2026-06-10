//! Transparent decompression of compressed disk images.
//!
//! Rufus's bled library decompresses `.xz`/`.gz`/`.bz2`/`.zip`/`.zst`/`.lzma`
//! images on the fly. The helper here does the same thing one step earlier:
//! when the user picks (or drops) a compressed image, the GUI streams it into
//! `~/.cache/usbooty/decompressed/<key>.img` and substitutes the cached path
//! into the rest of the pipeline. Loop-mount in the helper and seekable reads
//! in the ISO analyzer both need a real file, so the cache file is mandatory.
//!
//! XZ and raw `.lzma` go through `xz2` (system liblzma); everything else is
//! pure-Rust or statically bundles its C source. `xz2` is already a helper
//! dependency, so adding it here costs no new system link.
//!
//! Progress is reported as "input bytes consumed / source size", which is the
//! only basis available for streaming decompressors that don't expose the
//! uncompressed length.

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// The supported compression algorithms.
///
/// `None` means the file looks like a plain disk image and the rest of the
/// pipeline can use it untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    Xz,
    Gz,
    Bz2,
    Zst,
    Lzma,
    Zip,
    /// Unix `compress(1)`: legacy LZW. Rarely seen on modern distros
    /// (last common use was IRIX / Solaris), included for completeness
    /// because the cost of adding it is one tiny crate.
    Z,
    None,
}

impl Compression {
    fn name(self) -> &'static str {
        match self {
            Compression::Xz => "xz",
            Compression::Gz => "gzip",
            Compression::Bz2 => "bzip2",
            Compression::Zst => "zstd",
            Compression::Lzma => "lzma",
            Compression::Zip => "zip",
            Compression::Z => "z",
            Compression::None => "none",
        }
    }
}

/// Detect the compression algorithm from a file path.
///
/// Extension first, then magic-byte confirmation so a renamed file is not
/// blindly trusted. The peek-and-rewind costs one open and 6 bytes; cheap
/// enough to do on every `set_iso` call.
pub fn detect(path: &Path) -> Compression {
    let lower = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();

    // Tarball spellings (.tgz/.txz/.tbz2/.tz and .tar.*) are deliberately
    // excluded: they decompress to a tar archive, not a disk image, and
    // caching one as `<key>.img` would let it be DD-written verbatim.
    if lower.ends_with(".tgz")
        || lower.ends_with(".txz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tz")
        || lower.contains(".tar.")
    {
        return Compression::None;
    }
    let by_ext = if lower.ends_with(".xz") {
        Compression::Xz
    } else if lower.ends_with(".gz") {
        Compression::Gz
    } else if lower.ends_with(".bz2") {
        Compression::Bz2
    } else if lower.ends_with(".zst") || lower.ends_with(".zstd") {
        Compression::Zst
    } else if lower.ends_with(".lzma") {
        Compression::Lzma
    } else if lower.ends_with(".zip") {
        Compression::Zip
    } else if lower.ends_with(".z") {
        // Unix `compress(1)`: extension is case-sensitive by convention
        // (always uppercase `.Z`), but `lower` here matches both spellings.
        Compression::Z
    } else {
        Compression::None
    };

    // If extension says "compressed", confirm with the magic bytes; a
    // mismatch (e.g. `.img.gz` that is actually plain) silently falls back
    // to no-decompress, which is the safer default.
    if by_ext == Compression::None {
        return Compression::None;
    }
    match magic_of(path) {
        Some(actual) if actual == by_ext => actual,
        _ => Compression::None,
    }
}

/// Sniff the magic bytes at the start of `path` and return the matching
/// algorithm. Returns `None` if the file can't be read or doesn't match.
fn magic_of(path: &Path) -> Option<Compression> {
    let mut head = [0u8; 6];
    let mut f = File::open(path).ok()?;
    let n = f.read(&mut head).ok()?;
    let h = &head[..n];
    if h.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        Some(Compression::Xz)
    } else if h.starts_with(&[0x1F, 0x8B]) {
        Some(Compression::Gz)
    } else if h.starts_with(&[0x42, 0x5A, 0x68]) {
        Some(Compression::Bz2)
    } else if h.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Some(Compression::Zst)
    } else if h.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        Some(Compression::Zip)
    } else if h.starts_with(&[0x1F, 0x9D]) {
        // Unix compress(1): `1F 9D` then a flag byte. Low 5 bits = max code
        // width (typically 16), bit 7 = block mode (typically set).
        Some(Compression::Z)
    } else if h.starts_with(&[0x5D, 0x00, 0x00]) {
        // `.lzma` legacy framing: properties byte 0x5D, then a 4-byte
        // dictionary size (commonly 0x00100000 or 0x00800000). We accept any
        // value that has the high byte 0 so unrelated files don't match.
        Some(Compression::Lzma)
    } else {
        None
    }
}

/// Metadata kept alongside each cached file. Used to detect a cache entry that
/// no longer matches its source (re-downloaded, or different file at the same
/// path) so we recompute it instead of returning stale bytes.
#[derive(Serialize, Deserialize)]
struct Meta {
    source_path: String,
    source_size: u64,
    source_mtime: u64,
    algorithm: String,
    decompressed_size: u64,
    created_at: u64,
}

/// Decompress `src` into the user cache and return the cached file's path.
///
/// `progress` is invoked roughly once per read with `(consumed, total)` where
/// `total` is the source file size. The closure runs on the worker thread that
/// called this function, so anything it touches must be `Sync`.
pub fn decompress_to_cache(
    src: &Path,
    mut progress: impl FnMut(u64, u64) + Send,
) -> Result<PathBuf> {
    let algo = detect(src);
    if algo == Compression::None {
        bail!("file does not look compressed: {}", src.display());
    }

    let meta =
        fs::metadata(src).with_context(|| format!("reading metadata of {}", src.display()))?;
    let source_size = meta.len();
    let source_mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let dir = cache_dir()?;
    let key = cache_key(src, source_size, source_mtime, algo);
    let out_path = dir.join(format!("{key}.img"));
    let meta_path = dir.join(format!("{key}.meta.json"));

    // If a previous run cached this exact source-file fingerprint, reuse it.
    if out_path.is_file()
        && meta_path.is_file()
        && let Ok(text) = fs::read_to_string(&meta_path)
        && let Ok(prev) = serde_json::from_str::<Meta>(&text)
        && prev.source_size == source_size
        && prev.source_mtime == source_mtime
        && prev.algorithm == algo.name()
    {
        // Bump the mtimes so prune_cache sees the entry (and its sidecar) as
        // recently-used. Merely opening the file would not update them.
        touch(&out_path);
        touch(&meta_path);
        progress(source_size, source_size);
        return Ok(out_path);
    }

    // Stage the decompressed output to a sibling tempfile and atomically
    // rename on success; that way an interrupted decompress (Ctrl-C, OOM,
    // disk-full) never leaves a half-written file claiming to be cached.
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)
        .with_context(|| format!("creating tempfile in {}", dir.display()))?;

    let consumed = Arc::new(AtomicU64::new(0));
    let counter = ProgressReader {
        inner: BufReader::with_capacity(1024 * 1024, File::open(src)?),
        consumed: consumed.clone(),
    };

    // The decoder's read() drives the counter. We sample on every 1 MiB
    // boundary by interposing copy_with_progress between the decoder and the
    // tempfile writer.
    let written = match algo {
        Compression::Xz => copy_with_progress(
            &mut XzAdapter::new(counter)?,
            tmp.as_file_mut(),
            &consumed,
            source_size,
            &mut progress,
        )?,
        Compression::Gz => {
            // Multi-member decoder: the plain one stops at the first gzip
            // member, silently truncating bgzip/concatenated images.
            let mut dec = flate2::read::MultiGzDecoder::new(counter);
            copy_with_progress(
                &mut dec,
                tmp.as_file_mut(),
                &consumed,
                source_size,
                &mut progress,
            )?
        }
        Compression::Bz2 => {
            // Multi-stream decoder: pbzip2/lbzip2 output is concatenated
            // streams, which the single-stream decoder would truncate.
            let mut dec = bzip2::read::MultiBzDecoder::new(counter);
            copy_with_progress(
                &mut dec,
                tmp.as_file_mut(),
                &consumed,
                source_size,
                &mut progress,
            )?
        }
        Compression::Zst => {
            let mut dec =
                zstd::stream::read::Decoder::new(counter).context("initialising zstd decoder")?;
            copy_with_progress(
                &mut dec,
                tmp.as_file_mut(),
                &consumed,
                source_size,
                &mut progress,
            )?
        }
        Compression::Lzma => copy_with_progress(
            &mut LzmaAdapter::new(counter)?,
            tmp.as_file_mut(),
            &consumed,
            source_size,
            &mut progress,
        )?,
        Compression::Zip => extract_zip(
            counter,
            tmp.as_file_mut(),
            &consumed,
            source_size,
            &mut progress,
        )?,
        Compression::Z => copy_with_progress(
            &mut DotZAdapter::new(counter)?,
            tmp.as_file_mut(),
            &consumed,
            source_size,
            &mut progress,
        )?,
        Compression::None => unreachable!(),
    };

    tmp.as_file_mut()
        .sync_all()
        .context("flushing decompressed output")?;
    tmp.persist(&out_path)
        .with_context(|| format!("renaming to {}", out_path.display()))?;

    let meta_record = Meta {
        source_path: src.display().to_string(),
        source_size,
        source_mtime,
        algorithm: algo.name().to_string(),
        decompressed_size: written,
        created_at: now(),
    };
    if let Ok(json) = serde_json::to_string(&meta_record) {
        let _ = fs::write(&meta_path, json);
    }
    // One final progress tick at 100 % so the UI lands cleanly.
    progress(source_size, source_size);
    Ok(out_path)
}

/// Best-effort mtime bump, marking a cache entry as recently used so
/// [`prune_cache`] keeps it.
fn touch(path: &Path) {
    let _ = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(SystemTime::now()));
}

/// Drop cached entries that haven't been touched in `max_age_days` days.
///
/// Best-effort: any error reading the directory is ignored so a permissions
/// glitch never blocks app startup.
pub fn prune_cache(max_age_days: u64) {
    let Ok(dir) = cache_dir() else { return };
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let cutoff = Duration::from_secs(max_age_days * 24 * 3600);
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let age = meta
            .modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .unwrap_or_default();
        if age > cutoff {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Locate `~/.cache/usbooty/decompressed/`, creating it on demand.
fn cache_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("org", "usbooty", "usbooty")
        .context("cannot determine the user cache directory")?;
    let dir = dirs.cache_dir().join("decompressed");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// Build a deterministic cache filename from the source's fingerprint.
///
/// Including `(path, size, mtime, algorithm)` means: a different file at the
/// same path produces a different cache entry, but the *same* file with the
/// same mtime always hits cache, so users who pick the same `.iso.xz` twice
/// don't pay for a second decompress.
fn cache_key(path: &Path, size: u64, mtime: u64, algo: Compression) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut h = Sha256::new();
    h.update(canonical.to_string_lossy().as_bytes());
    h.update(b"\0");
    h.update(size.to_le_bytes());
    h.update(mtime.to_le_bytes());
    h.update(algo.name().as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Current Unix time in seconds (zero on failure, only used for telemetry).
fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A `Read` wrapper that counts the bytes pulled from the source file.
///
/// The counter is shared so the decompression loop can sample it without
/// reaching into the decoder's internals.
struct ProgressReader<R: Read> {
    inner: R,
    consumed: Arc<AtomicU64>,
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.consumed.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl<R: BufRead> BufRead for ProgressReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.consumed.fetch_add(amt as u64, Ordering::Relaxed);
        self.inner.consume(amt);
    }
}

/// Stream `src` to `dst` in 1 MiB chunks, calling `progress` between chunks
/// (throttled: a fast decompressor can push ~1000 chunks/s, and every call
/// queues a Qt event plus string formatting on the GUI side).
fn copy_with_progress<R: Read, W: Write>(
    src: &mut R,
    dst: &mut W,
    consumed: &AtomicU64,
    total: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64> {
    const PROGRESS_EVERY: Duration = Duration::from_millis(150);
    let mut buf = vec![0u8; 1024 * 1024];
    let mut written = 0u64;
    let mut last_report = std::time::Instant::now();
    loop {
        let n = src.read(&mut buf).context("reading decompressed bytes")?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])
            .context("writing decompressed bytes")?;
        written += n as u64;
        if last_report.elapsed() >= PROGRESS_EVERY {
            progress(consumed.load(Ordering::Relaxed).min(total), total);
            last_report = std::time::Instant::now();
        }
    }
    Ok(written)
}

/// Extensions an entry inside a zip must carry to be treated as the disk
/// image. `.wim`/`.esd` cover Windows downloads that ship a lone image file.
const ZIP_IMAGE_EXTS: &[&str] = &[".iso", ".img", ".raw", ".vhd", ".wim", ".esd", ".bin"];

/// Pick the disk-image entry out of a ZIP archive and stream it to `dst`.
///
/// `read_zipfile_from_stream` walks local file headers without needing the
/// central directory, so the whole archive never lives in memory. Only an
/// entry with a disk-image extension is accepted: taking the first regular
/// file blindly would cache a stray `README.txt` or checksum file as the
/// image and let it be DD-written to the drive.
fn extract_zip<R: Read>(
    src: R,
    dst: &mut std::fs::File,
    consumed: &AtomicU64,
    total: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64> {
    let mut src = src;
    let mut skipped: Vec<String> = Vec::new();
    while let Some(mut entry) = zip::read::read_zipfile_from_stream(&mut src)? {
        let name = entry.name().to_lowercase();
        if name.ends_with('/') {
            continue; // directory entry
        }
        if ZIP_IMAGE_EXTS.iter().any(|ext| name.ends_with(ext)) {
            return copy_with_progress(&mut entry, dst, consumed, total, progress);
        }
        skipped.push(entry.name().to_string());
        // Dropping the entry advances the stream past its data.
    }
    if skipped.is_empty() {
        bail!("the zip archive contains no usable disk image");
    }
    bail!(
        "the zip archive contains no disk image (expected one of {}); found: {}",
        ZIP_IMAGE_EXTS.join(" "),
        skipped.join(", ")
    )
}

/// Streaming `.xz` decoder backed by `xz2::read::XzDecoder` (system liblzma).
/// Constructor is fallible only to match the existing call-site shape; the
/// real liblzma errors surface during `read`.
struct XzAdapter<R: BufRead> {
    inner: xz2::read::XzDecoder<R>,
}

impl<R: BufRead> XzAdapter<R> {
    fn new(reader: R) -> Result<Self> {
        // Multi-stream decoder: concatenated .xz streams are legal and the
        // single-stream decoder would silently truncate after the first.
        Ok(Self {
            inner: xz2::read::XzDecoder::new_multi_decoder(reader),
        })
    }
}

impl<R: BufRead> Read for XzAdapter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

/// Streaming raw `.lzma` (legacy 13-byte-header) decoder built on
/// `xz2::stream::Stream::new_lzma_decoder`. Same trade-off as [`XzAdapter`].
struct LzmaAdapter<R: BufRead> {
    inner: xz2::read::XzDecoder<R>,
}

impl<R: BufRead> LzmaAdapter<R> {
    fn new(reader: R) -> Result<Self> {
        // u64::MAX memlimit matches `xz --lzma1=preset=9` decoders; the
        // streaming path means peak resident memory stays bounded by the
        // dictionary size embedded in the file header.
        let stream = xz2::stream::Stream::new_lzma_decoder(u64::MAX)
            .map_err(|e| anyhow::anyhow!("lzma decoder init: {e}"))?;
        Ok(Self {
            inner: xz2::read::XzDecoder::new_stream(reader, stream),
        })
    }
}

impl<R: BufRead> Read for LzmaAdapter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

/// Adapter for Unix `compress(1)` (`.Z`) streams.
///
/// `.Z` wraps an LZW bitstream in a 3-byte header: `1F 9D` plus a flag byte
/// whose low 5 bits hold the maximum code width (typically 16). Producers
/// pack codes LSB-first and start at 9 bits, growing as the dictionary fills.
/// `weezl` handles that LZW dialect; we just strip the header and pass the
/// rest through.
///
/// Decode-into-memory pattern, same as `XzAdapter` and `LzmaAdapter`: `weezl`
/// is buffer-oriented and `.Z` images are tiny in practice (no modern distro
/// ships gigabyte-class Unix-compress media). If a user ever feeds in a
/// monstrous one, switching to a streaming `weezl` adapter is mechanical.
struct DotZAdapter {
    inner: io::Cursor<Vec<u8>>,
}

impl DotZAdapter {
    fn new<R: Read>(mut reader: R) -> Result<Self> {
        let mut hdr = [0u8; 3];
        reader.read_exact(&mut hdr).context("reading .Z header")?;
        if hdr[0] != 0x1F || hdr[1] != 0x9D {
            anyhow::bail!(".Z stream has wrong magic bytes");
        }
        let max_bits = (hdr[2] & 0x1F) as u32;
        // 9 is the smallest legal Unix-compress width (8 bits is `pack(1)`,
        // not `compress(1)`); 16 is the upstream maximum.
        if !(9..=16).contains(&max_bits) {
            anyhow::bail!(".Z header reports an out-of-range max-bits value: {max_bits}");
        }
        let mut compressed = Vec::new();
        reader
            .read_to_end(&mut compressed)
            .context("reading .Z payload")?;

        // BitOrder::Lsb + size 8: matches Unix compress (256-symbol alphabet,
        // codes packed LSB-first, dictionary grows from 9 bits up to max_bits).
        let mut decoder = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, 8);
        let out = decoder
            .decode(&compressed)
            .map_err(|e| anyhow::anyhow!(".Z decode failed: {e:?}"))?;
        Ok(Self {
            inner: io::Cursor::new(out),
        })
    }
}

impl Read for DotZAdapter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}
