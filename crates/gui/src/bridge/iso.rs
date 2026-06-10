//! `AppController` methods for the source image: selection, analysis hand-off,
//! clearing, and on-demand digest computation.

use std::path::PathBuf;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::{IsoReport, OsKind};

use super::qobject;

impl qobject::AppController {
    /// Set the source ISO (normalizing a `file://` URL) and analyze it.
    ///
    /// A compressed source (`.xz`/`.gz`/`.bz2`/`.zst`/`.lzma`/`.zip`) is
    /// transparently decompressed to `~/.cache/usbooty/decompressed/` first;
    /// that runs on a worker thread so the UI stays responsive while many
    /// gigabytes stream through. The plain-ISO fast path is unchanged.
    pub fn set_iso(mut self: core::pin::Pin<&mut Self>, path: &QString) {
        let path = super::helpers::local_path_from_url(&path.to_string());
        if path.is_empty() {
            return;
        }
        let path_buf = PathBuf::from(&path);
        if !path_buf.is_file() {
            self.as_mut()
                .set_iso_summary(QString::from("Cannot read that file"));
            return;
        }
        match crate::decompress::detect(&path_buf) {
            crate::decompress::Compression::None => {
                // A `.vhd` file is raw data + a 512-byte footer; strip the
                // footer to a cache file and re-enter with the plain image.
                // Dynamic / differencing VHDs surface an error to the UI.
                if path_buf
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("vhd"))
                {
                    self.as_mut().set_busy(true);
                    self.as_mut().set_progress(0.0);
                    self.as_mut().set_phase(QString::from("Unwrapping VHD"));
                    self.as_mut()
                        .set_iso_summary(QString::from("Unwrapping fixed VHD…"));
                    self.as_mut().set_iso_sha256(QString::default());
                    self.as_mut().set_iso_path(QString::default());
                    self.as_mut()
                        .set_status(QString::from("Unwrapping fixed VHD…"));
                    let qt = self.qt_thread();
                    std::thread::spawn(move || {
                        crate::runner::strip_vhd_then_analyze(qt, path_buf);
                    });
                    return;
                }
                // `iso::analyze` can FUSE-mount or ISO9660-parse a multi-GB
                // file, which is hundreds of milliseconds. Background-thread
                // it so the Qt event loop keeps spinning while the file is
                // picked.
                self.as_mut().set_busy(true);
                self.as_mut().set_progress(0.0);
                self.as_mut().set_phase(QString::from("Analyzing"));
                self.as_mut()
                    .set_iso_summary(QString::from("Analyzing source image…"));
                self.as_mut()
                    .set_status(QString::from("Analyzing source image…"));
                let qt = self.qt_thread();
                std::thread::spawn(move || {
                    crate::runner::analyze_then_apply(qt, path_buf);
                });
            }
            _ => {
                self.as_mut().set_busy(true);
                self.as_mut().set_progress(0.0);
                self.as_mut().set_phase(QString::from("Decompressing"));
                self.as_mut()
                    .set_iso_summary(QString::from("Decompressing source image…"));
                self.as_mut().set_iso_sha256(QString::default());
                self.as_mut().set_iso_path(QString::default());
                self.as_mut()
                    .set_status(QString::from("Decompressing source image…"));
                let qt = self.qt_thread();
                std::thread::spawn(move || {
                    crate::runner::decompress_then_analyze(qt, path_buf);
                });
            }
        }
    }

    /// Reset every field derived from the source ISO so the slot looks
    /// "fresh" again. Used by the *Clear source image* menu entry; the
    /// inverse of `apply_iso`.
    pub fn clear_iso(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().set_iso_path(QString::default());
        self.as_mut()
            .set_iso_summary(QString::from("No image selected"));
        self.as_mut().set_label(QString::default());
        self.as_mut().set_windows_iso(false);
        self.as_mut().set_linux_iso(false);
        self.as_mut().set_persistence_supported(false);
        self.as_mut().set_persistence_inline(false);
        self.as_mut().set_distro_label(QString::default());
        self.as_mut().set_persistence_size(0);
        self.as_mut().set_iso_md5(QString::default());
        self.as_mut().set_iso_sha1(QString::default());
        self.as_mut().set_iso_sha256(QString::default());
        self.as_mut().set_iso_sha512(QString::default());
        self.as_mut().set_iso_blake3(QString::default());
        self.as_mut().set_iso_adguard_badge(QString::default());
        self.as_mut().set_hash_progress(0.0);
        self.as_mut().set_revocation_warnings(QString::default());
        // Invalidate any in-flight hash worker and stop the spinners; its
        // results belong to the ISO that was just cleared.
        self.as_mut().rust_mut().hash_generation += 1;
        self.as_mut().set_hashing(false);
        self.as_mut().rust_mut().iso_report = None;
        self.as_mut().refresh_fit_warning();
        self.as_mut().refresh_persistence_max();
    }

    /// Apply an analyzed ISO to the UI state. When `hashes` is `Some` the
    /// digests are already known (a downloaded ISO); otherwise they are
    /// computed off-thread. `win` is the WIM metadata pre-computed by the
    /// analysis worker (parsing it here would block the Qt thread on a
    /// multi-megabyte disk read). Marked `pub(crate)` so [`crate::runner`]
    /// can call this from a Qt-thread closure after off-thread analysis.
    pub(crate) fn apply_iso(
        mut self: core::pin::Pin<&mut Self>,
        path: &str,
        report: IsoReport,
        win: Option<crate::iso::WindowsMeta>,
        hashes: Option<&crate::iso::IsoHashes>,
    ) {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        let summary = format!("{name}  ·  {}", report.summary());
        let vol_label = report.label.clone();
        let pers_supported = report.persistence.is_some();
        let pers_inline = report
            .persistence
            .map(|k| !k.needs_partition())
            .unwrap_or(false);
        let distro_label = if report.distro == usbooty_core::DistroFamily::Unknown {
            String::new()
        } else {
            report.distro.display().to_string()
        };
        let is_windows = report.os_kind == OsKind::Windows;
        let is_linux = report.os_kind == OsKind::Linux;

        // A different ISO is taking over: any hash worker still running for
        // the previous one must not publish onto this one's panel.
        self.as_mut().rust_mut().hash_generation += 1;
        self.as_mut().set_hashing(false);

        self.as_mut().set_iso_path(QString::from(path));
        self.as_mut().set_iso_summary(QString::from(&summary));
        // Pre-fill the editable volume label from the image's own label.
        self.as_mut().set_label(QString::from(&vol_label));
        self.as_mut().set_persistence_supported(pers_supported);
        self.as_mut().set_persistence_inline(pers_inline);
        self.as_mut().set_distro_label(QString::from(&distro_label));
        self.as_mut().set_persistence_size(0);
        self.as_mut().set_windows_iso(is_windows);
        self.as_mut().set_linux_iso(is_linux);
        // The install.wim build number gates version-specific installer options
        // in QML (Windows 11 is build >= 22000); the arch lets the unattend
        // target one architecture instead of emitting all three. Both come
        // pre-computed from the worker; 0 / empty for non-Windows or unknown.
        let win = if is_windows { win.unwrap_or_default() } else { Default::default() };
        self.as_mut().set_windows_build(win.build as i32);
        self.as_mut()
            .set_windows_arch(QString::from(&win.arch.unwrap_or_default()));

        // Auto-pick the write method the image needs: the partition method for
        // a Windows/Linux installer, raw DD for a BSD/other image (DD is
        // OS-agnostic and the only method that boots those). Leave explicit
        // "Format only" / "Ventoy" choices alone; the user can still override.
        if *self.method() < 2 {
            let auto_method = if is_windows || is_linux { 1 } else { 0 };
            self.as_mut().set_method(auto_method);
        }
        let rev_text = report.revocation_warnings.join("\n");
        self.as_mut()
            .set_revocation_warnings(QString::from(&rev_text));
        self.as_mut().rust_mut().iso_report = Some(report);

        match hashes {
            // Downloaded ISO: every digest was computed during the download.
            Some(h) => {
                self.as_mut().set_iso_md5(QString::from(&h.md5));
                self.as_mut().set_iso_sha1(QString::from(&h.sha1));
                self.as_mut().set_iso_sha256(QString::from(&h.sha256));
                self.as_mut().set_iso_sha512(QString::from(&h.sha512));
                self.as_mut().set_iso_blake3(QString::from(&h.blake3));
            }
            // Local ISO: hashing a multi-gigabyte file is CPU-heavy (five
            // hashers updated per chunk), so leave the digests blank until
            // the user explicitly asks for them via `compute_hashes()`.
            None => {
                self.as_mut().set_iso_md5(QString::default());
                self.as_mut().set_iso_sha1(QString::default());
                self.as_mut().set_iso_sha256(QString::default());
                self.as_mut().set_iso_sha512(QString::default());
                self.as_mut().set_iso_blake3(QString::default());
            }
        }

        self.as_mut().refresh_fit_warning();
        self.as_mut().refresh_persistence_max();
    }

    /// Kick off off-thread digest computation for the currently-loaded ISO.
    /// Clears the digest fields and sets `hashing` so the panel shows a spinner
    /// per hash; the worker fills each value in as its thread finishes, and
    /// `hash_progress` tracks the shared read pass for the percentage.
    pub fn compute_hashes(mut self: core::pin::Pin<&mut Self>) {
        let path = self.iso_path().to_string();
        if path.is_empty() {
            return;
        }
        // Clear any previous digests and flip into the spinner-per-hash state;
        // the worker fills each value in as its thread finishes.
        self.as_mut().set_iso_md5(QString::default());
        self.as_mut().set_iso_sha1(QString::default());
        self.as_mut().set_iso_sha256(QString::default());
        self.as_mut().set_iso_sha512(QString::default());
        self.as_mut().set_iso_blake3(QString::default());
        self.as_mut().set_iso_adguard_badge(QString::default());
        self.as_mut().set_hash_progress(0.0);
        self.as_mut().set_hashing(true);

        // Bind the worker to the current generation: bumping it (new ISO
        // loaded, Compute clicked again) makes this worker's queued closures
        // no-ops instead of publishing stale digests.
        let generation = {
            let mut rust = self.as_mut().rust_mut();
            rust.hash_generation += 1;
            rust.hash_generation
        };
        let qt = self.qt_thread();
        std::thread::spawn(move || crate::runner::compute_iso_hashes(qt, path, generation));
    }
}
