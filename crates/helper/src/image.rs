//! Opening source images for the DD path, including transparent decompression.
//!
//! Many distros ship their installer ISOs pre-compressed (`.iso.xz`,
//! `.iso.zst`, `.img.gz`, `.iso.bz2`); the user expects to point usbooty at the
//! file as-downloaded and have it written. The DD path is the only mode that
//! benefits from compression (Partitioned mode loop-mounts the ISO, which the
//! kernel does not do for compressed images), so the streaming lives here.
//!
//! Progress is reported in compressed-input bytes; that is the value the
//! kernel actually reads from disk, so reporting it gives a smooth, accurate
//! bar regardless of how the data unpacks. The decompressed size is generally
//! unknown until we are done (gzip's ISIZE field is mod 2^32, xz/zstd carry it
//! but only in some frames, bz2 not at all), so we cannot fit-check up front;
//! the write will fail cleanly if the device is too small.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::vhd;

/// A source image opened for streaming.
pub struct Image {
    /// Stream of *decompressed* bytes; for a raw image this is just the file.
    pub reader: Box<dyn Read + Send>,
    /// Size of the file on disk (compressed or not), the basis for progress.
    pub compressed_size: u64,
    /// How many bytes the reader has pulled from the underlying file so far.
    /// Distinct from "bytes produced": for a compressed image, this advances
    /// proportionally with disk I/O, which is what the user wants to watch.
    pub bytes_read: Arc<AtomicU64>,
    /// True iff the input was a compressed stream. The fit-check has to be
    /// skipped in that case (decompressed size unknown).
    pub compressed: bool,
}

/// Open `path` for streaming, transparently decompressing if the extension is
/// one of the supported compressed formats. The extension is taken from the
/// *outermost* part of the filename only; `linux.iso.xz` is xz-decompressed
/// once, yielding the raw ISO bytes.
pub fn open(path: &Path) -> Result<Image> {
    let file = File::open(path).with_context(|| format!("opening image {}", path.display()))?;
    let compressed_size = file.metadata().context("stat image")?.len();
    let bytes_read = Arc::new(AtomicU64::new(0));
    let counter = ByteCounter::new(file, bytes_read.clone());

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let (reader, compressed): (Box<dyn Read + Send>, bool) = match ext.as_str() {
        "gz" => (Box::new(flate2::read::GzDecoder::new(counter)), true),
        "xz" => (Box::new(xz2::read::XzDecoder::new(counter)), true),
        "zst" | "zstd" => (
            Box::new(zstd::stream::read::Decoder::new(counter).context("zstd init")?),
            true,
        ),
        "bz2" => (Box::new(bzip2::read::BzDecoder::new(counter)), true),
        // VHD: virtual disk size differs from file size, but we still know it
        // up front so the fit-check works. The byte counter is dropped here
        // because the VHD reader does its own seeking; progress tracks the
        // bytes produced (= virtual disk position) by an inline counter below.
        "vhd" => {
            let virt = vhd::VhdReader::new(counter.into_inner()).context("opening VHD")?;
            let virt_size = virt.virtual_size();
            return Ok(Image {
                reader: Box::new(VirtualSizeCounter::new(virt, bytes_read.clone())),
                compressed_size: virt_size,
                bytes_read,
                compressed: false,
            });
        }
        "vhdx" => return Err(vhd::refuse_vhdx()),
        _ => (Box::new(counter), false),
    };

    Ok(Image {
        reader,
        compressed_size,
        bytes_read,
        compressed,
    })
}

/// `Read` wrapper that records how many bytes pass through the underlying
/// reader, into a shared atomic. Used so the progress bar can advance against
/// disk reads even when the data is being decompressed on the fly.
struct ByteCounter<R: Read> {
    inner: R,
    counter: Arc<AtomicU64>,
}

impl<R: Read> ByteCounter<R> {
    fn new(inner: R, counter: Arc<AtomicU64>) -> Self {
        Self { inner, counter }
    }

    /// Unwrap the counter, discarding the count atomic. Used when a wrapping
    /// reader takes over progress tracking on its own terms (e.g. VHD, which
    /// counts virtual disk position rather than raw file reads).
    fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for ByteCounter<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// A `Read` wrapper that records how many bytes have been *produced* (not
/// just read from disk) into a shared counter. Used for VHD, where the
/// virtual disk size, not the file size, is what the user expects the
/// progress bar to track against.
struct VirtualSizeCounter<R: Read> {
    inner: R,
    counter: Arc<AtomicU64>,
}

impl<R: Read> VirtualSizeCounter<R> {
    fn new(inner: R, counter: Arc<AtomicU64>) -> Self {
        Self { inner, counter }
    }
}

impl<R: Read> Read for VirtualSizeCounter<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}
