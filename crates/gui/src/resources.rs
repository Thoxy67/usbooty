//! Runtime fetcher for resource files taken from the Rufus repository.
//!
//! These files (currently just the UEFI:NTFS bootloader image) are never
//! vendored into usbooty. They are downloaded on demand from the Rufus GitHub
//! raw URLs and cached under `~/.cache/usbooty/resources/`, so the app always
//! tracks the latest upstream version while still working offline once cached.

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a cached resource is trusted before a fresh check is made.
const TTL: Duration = Duration::from_secs(7 * 24 * 3600);

/// A resource that can be fetched from the Rufus repository.
#[derive(Clone, Copy, Debug)]
pub enum Resource {
    /// The 1 MiB FAT32 image carrying the UEFI:NTFS bootloader.
    UefiNtfsImg,
}

impl Resource {
    /// The cached file name.
    fn name(self) -> &'static str {
        match self {
            Resource::UefiNtfsImg => "uefi-ntfs.img",
        }
    }

    /// The upstream raw URL (always tracks the Rufus `master` branch).
    fn url(self) -> &'static str {
        match self {
            Resource::UefiNtfsImg => {
                "https://raw.githubusercontent.com/pbatard/rufus/master/res/uefi/uefi-ntfs.img"
            }
        }
    }

    /// Structurally validate the bytes of this resource — the download is never
    /// integrity-checked by the transport, so a truncated or corrupted file
    /// must be rejected before it is cached or written to a drive.
    fn validate(self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Resource::UefiNtfsImg => usbooty_core::validate_uefi_ntfs(bytes),
        }
    }
}

/// Cached HTTP validators, stored alongside each resource as `<name>.meta`.
#[derive(Serialize, Deserialize, Default)]
struct Meta {
    etag: Option<String>,
    fetched_at: u64,
    size: u64,
}

/// Ensure `resource` is available locally and return its path.
///
/// If a fresh cached copy exists it is returned immediately. Otherwise a
/// (conditional) download is attempted; on a network failure a stale cached
/// copy is used if one exists, and only a complete absence of any copy is a
/// hard error. This function blocks and must run off the Qt thread.
pub fn ensure(resource: Resource) -> Result<PathBuf> {
    let dir = cache_dir()?;
    let file = dir.join(resource.name());
    let meta_path = dir.join(format!("{}.meta", resource.name()));
    let meta: Meta = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // A fresh *and structurally valid* cached copy needs no network at all.
    if now().saturating_sub(meta.fetched_at) < TTL.as_secs() && cached_valid(resource, &file) {
        return Ok(file);
    }

    match download(resource, meta.etag.as_deref()) {
        Ok(Fetch::NotModified) => {
            // The server says our copy is current — trust it only if the bytes
            // on disk still validate (a cache file can rot independently).
            if cached_valid(resource, &file) {
                save_meta(
                    &meta_path,
                    &Meta {
                        fetched_at: now(),
                        ..meta
                    },
                );
                Ok(file)
            } else {
                bail!(
                    "the cached {} is corrupt and the server reports no newer copy",
                    resource.name()
                );
            }
        }
        Ok(Fetch::Body { bytes, etag }) => {
            if let Err(why) = resource.validate(&bytes) {
                bail!("the downloaded {} is invalid: {why}", resource.name());
            }
            std::fs::write(&file, &bytes).with_context(|| format!("writing {}", file.display()))?;
            save_meta(
                &meta_path,
                &Meta {
                    etag,
                    fetched_at: now(),
                    size: bytes.len() as u64,
                },
            );
            Ok(file)
        }
        Err(e) => {
            // Offline: fall back to the cached copy only if it is still valid.
            if cached_valid(resource, &file) {
                Ok(file)
            } else {
                Err(e).with_context(|| {
                    format!(
                        "could not download {} and no valid cached copy exists",
                        resource.name()
                    )
                })
            }
        }
    }
}

/// Whether `file` exists and holds a structurally-valid copy of `resource`.
fn cached_valid(resource: Resource, file: &Path) -> bool {
    std::fs::read(file)
        .ok()
        .is_some_and(|bytes| resource.validate(&bytes).is_ok())
}

/// Outcome of a conditional download.
enum Fetch {
    /// Server replied 304 — the cached copy is current.
    NotModified,
    /// A fresh body was downloaded.
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

/// Perform a conditional HTTP GET for `resource`.
fn download(resource: Resource, etag: Option<&str>) -> Result<Fetch> {
    let mut request = ureq::get(resource.url());
    if let Some(tag) = etag {
        request = request.header("If-None-Match", tag);
    }

    match request.call() {
        Ok(mut response) => {
            let new_etag = response
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let bytes = response
                .body_mut()
                .read_to_vec()
                .context("reading the downloaded body")?;
            Ok(Fetch::Body {
                bytes,
                etag: new_etag,
            })
        }
        Err(ureq::Error::StatusCode(304)) => Ok(Fetch::NotModified),
        Err(e) => Err(e).context("HTTP request failed"),
    }
}

/// Locate (and create) `~/.cache/usbooty/resources/`.
fn cache_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("org", "usbooty", "usbooty")
        .context("cannot determine the user cache directory")?;
    let dir = dirs.cache_dir().join("resources");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// Current Unix time in seconds.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Best-effort write of the sidecar metadata file.
fn save_meta(path: &std::path::Path, meta: &Meta) {
    if let Ok(json) = serde_json::to_string(meta) {
        let _ = std::fs::write(path, json);
    }
}
