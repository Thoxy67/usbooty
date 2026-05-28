//! Runtime fetcher for resource files taken from the Rufus repository.
//!
//! These files (currently just the UEFI:NTFS bootloader image) are never
//! vendored into usbooty. They are downloaded on demand from the Rufus GitHub
//! raw URLs and cached under `~/.cache/usbooty/resources/`, so the app always
//! tracks the latest upstream version while still working offline once cached.

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

/// How long a cached resource is trusted before a fresh check is made.
const TTL: Duration = Duration::from_secs(7 * 24 * 3600);

/// A resource that can be fetched from the Rufus repository.
#[derive(Clone, Copy, Debug)]
pub enum Resource {
    /// The 1 MiB FAT32 image carrying the UEFI:NTFS bootloader.
    UefiNtfsImg,
    /// The signed UEFI Forum DBX revocation update for x86_64.
    DbxX64,
    /// The signed UEFI Forum DBX revocation update for ARM64.
    DbxArm64,
}

impl Resource {
    /// The cached file name.
    fn name(self) -> &'static str {
        match self {
            Resource::UefiNtfsImg => "uefi-ntfs.img",
            Resource::DbxX64 => "dbxupdate_x64.bin",
            Resource::DbxArm64 => "dbxupdate_arm64.bin",
        }
    }

    /// The upstream raw URL.
    fn url(self) -> &'static str {
        match self {
            Resource::UefiNtfsImg => {
                "https://raw.githubusercontent.com/pbatard/rufus/master/res/uefi/uefi-ntfs.img"
            }
            // UEFI Forum publishes the current DBX update files at well-known
            // permalinks; the bytes themselves are signed, so the URL doesn't
            // need to embed a version.
            Resource::DbxX64 => "https://uefi.org/sites/default/files/resources/dbxupdate_x64.bin",
            Resource::DbxArm64 => {
                "https://uefi.org/sites/default/files/resources/dbxupdate_arm64.bin"
            }
        }
    }

    /// Structurally validate the bytes of this resource — the download is never
    /// integrity-checked by the transport, so a truncated or corrupted file
    /// must be rejected before it is cached or written to a drive.
    fn validate(self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Resource::UefiNtfsImg => usbooty_core::validate_uefi_ntfs(bytes),
            // DBX update files vary by month (Microsoft re-signs them every
            // few weeks). The hard guarantee is that they're at least
            // 64 bytes — anything shorter can't even hold the auth header.
            Resource::DbxX64 | Resource::DbxArm64 => {
                if bytes.len() < 64 {
                    Err(format!(
                        "{}: only {} bytes (too small to be a DBX update)",
                        self.name(),
                        bytes.len()
                    ))
                } else {
                    Ok(())
                }
            }
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

/// Process-global cache of the revocation DB. Populated either by
/// [`prime_revocation_db`] at startup (which fetches the live DBX updates)
/// or lazily by [`cached_revocation_db`] (which returns just the baked-in
/// baseline when nothing has been primed yet).
static REVOCATION_DB: OnceLock<usbooty_core::revocation::RevocationDb> = OnceLock::new();

/// Augment `usbooty_core::RevocationDb::baked_in()` with whichever DBX
/// updates we can fetch from the UEFI Forum, then memoise the result in
/// the process-global cache so subsequent calls are free.
///
/// Fails soft on every network hop — a missing or unreachable update file
/// leaves the baked-in baseline intact, never blocks the SBAT scan.
/// Called by the GUI at startup, off the Qt thread.
pub fn prime_revocation_db() {
    let mut db = usbooty_core::revocation::RevocationDb::baked_in();
    for res in [Resource::DbxX64, Resource::DbxArm64] {
        if let Ok(path) = ensure(res)
            && let Ok(bytes) = std::fs::read(&path)
        {
            let extra = usbooty_core::revocation::parse_dbxupdate(&bytes);
            if !extra.0.is_empty() {
                db.extend_dbx(extra);
            }
        }
    }
    let _ = REVOCATION_DB.set(db);
}

/// Return the most-current revocation DB usbooty has. Falls back to the
/// baked-in baseline when [`prime_revocation_db`] has not run yet (e.g.
/// the user opened an ISO before the startup network call completed).
pub fn cached_revocation_db() -> usbooty_core::revocation::RevocationDb {
    REVOCATION_DB
        .get()
        .cloned()
        .unwrap_or_else(usbooty_core::revocation::RevocationDb::baked_in)
}

/// Best-effort write of the sidecar metadata file.
fn save_meta(path: &std::path::Path, meta: &Meta) {
    if let Ok(json) = serde_json::to_string(meta) {
        let _ = std::fs::write(path, json);
    }
}

// ---------------------------------------------------------------------------
// FreeDOS bootable-USB resources
// ---------------------------------------------------------------------------

/// GitHub repos providing the FreeDOS pieces we need. Their latest release
/// is resolved via the GitHub API at fetch time, so a fresh kernel or
/// command-shell release lands automatically without a code bump.
const FREEDOS_KERNEL_REPO: &str = "FDOS/kernel";
const FREEDOS_COMMAND_REPO: &str = "FDOS/command-com";

/// How long usbooty trusts the resolved "latest release" URL before asking
/// GitHub again. A day is fine — these projects ship at glacial speed and
/// the cached zips themselves are dimension-checked by their own metadata.
const LATEST_RELEASE_TTL: Duration = Duration::from_secs(24 * 3600);

/// Local file paths to the three pieces the helper needs for a FreeDOS USB:
/// the FAT boot sector matching the chosen FAT variant, the kernel, and the
/// command interpreter.
#[derive(Clone, Debug)]
pub struct FreedosFiles {
    pub kernel_sys: PathBuf,
    pub command_com: PathBuf,
    /// `BOOT16.BIN` or `BOOT32.BIN` depending on the FAT variant.
    pub boot_bin: PathBuf,
}

/// Fetch the upstream FreeDOS kernel and command-shell ZIPs (caching the
/// raw zips under `~/.cache/usbooty/resources/`), then extract the three
/// files the helper needs into stable per-file paths.
///
/// `fat32`: pick `BOOT32.BIN` when true, `BOOT16.BIN` otherwise.
///
/// The helper itself never touches the network — the GUI runs this off the
/// Qt thread and hands the resulting paths over via [`Job::Freedos`].
///
/// [`Job::Freedos`]: usbooty_core::Job::Freedos
pub fn ensure_freedos(fat32: bool) -> Result<FreedosFiles> {
    let dir = cache_dir()?;
    let extract_dir = dir.join("freedos");
    std::fs::create_dir_all(&extract_dir)
        .with_context(|| format!("creating {}", extract_dir.display()))?;

    // Resolve "latest zip" for each repo via the GitHub API (cached), then
    // conditionally fetch the actual zip.
    let kernel_url = latest_release_zip(FREEDOS_KERNEL_REPO)?;
    let command_url = latest_release_zip(FREEDOS_COMMAND_REPO)?;
    let kernel_zip = ensure_url(&kernel_url, "freedos-kernel.zip")?;
    let command_zip = ensure_url(&command_url, "freedos-command.zip")?;

    // Pull KERNEL.SYS + BOOT{16,32}.BIN out of the kernel zip.
    let want_boot = if fat32 { "BOOT32.BIN" } else { "BOOT16.BIN" };
    extract_named(
        &kernel_zip,
        &["BIN/KERNEL.SYS", "KERNEL.SYS"],
        &extract_dir.join("KERNEL.SYS"),
    )?;
    extract_named(
        &kernel_zip,
        &[&format!("BOOT/{want_boot}"), want_boot],
        &extract_dir.join(want_boot),
    )?;
    extract_named(
        &command_zip,
        &["COMMAND.COM", "BIN/COMMAND.COM"],
        &extract_dir.join("COMMAND.COM"),
    )?;

    Ok(FreedosFiles {
        kernel_sys: extract_dir.join("KERNEL.SYS"),
        command_com: extract_dir.join("COMMAND.COM"),
        boot_bin: extract_dir.join(want_boot),
    })
}

/// Generic conditional-GET caching for an arbitrary upstream URL. Same
/// shape as `ensure(Resource)` minus the structural validator.
fn ensure_url(url: &str, name: &str) -> Result<PathBuf> {
    let dir = cache_dir()?;
    let file = dir.join(name);
    let meta_path = dir.join(format!("{name}.meta"));
    let meta: Meta = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    if now().saturating_sub(meta.fetched_at) < TTL.as_secs() && file.is_file() {
        return Ok(file);
    }

    let mut req = ureq::get(url);
    if let Some(tag) = meta.etag.as_deref() {
        req = req.header("If-None-Match", tag);
    }
    match req.call() {
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
            std::fs::write(&file, &bytes).with_context(|| format!("writing {}", file.display()))?;
            save_meta(
                &meta_path,
                &Meta {
                    etag: new_etag,
                    fetched_at: now(),
                    size: bytes.len() as u64,
                },
            );
            Ok(file)
        }
        Err(ureq::Error::StatusCode(304)) if file.is_file() => Ok(file),
        Err(e) => {
            if file.is_file() {
                Ok(file)
            } else {
                Err(e).with_context(|| format!("downloading {url}"))
            }
        }
    }
}

/// Ask GitHub for `<repo>`'s latest release and return the first `.zip`
/// asset's download URL. Resolutions are cached under `<repo-slug>.url`
/// alongside the rest of the resources, so we hit the API at most once
/// per day — well under the 60/hour unauthenticated quota even if the
/// app is opened constantly.
fn latest_release_zip(repo: &str) -> Result<String> {
    let dir = cache_dir()?;
    let slug = repo.replace('/', "-");
    let cache_path = dir.join(format!("{slug}.url"));
    let stamp_path = dir.join(format!("{slug}.url.meta"));

    let meta: Meta = std::fs::read_to_string(&stamp_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if now().saturating_sub(meta.fetched_at) < LATEST_RELEASE_TTL.as_secs()
        && cache_path.is_file()
        && let Ok(url) = std::fs::read_to_string(&cache_path)
    {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let api = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut response = ureq::get(&api)
        // GitHub requires a User-Agent; ureq sets one by default but be explicit.
        .header("User-Agent", concat!("usbooty/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("querying {api}"))?;
    let body = response
        .body_mut()
        .read_to_vec()
        .context("reading GitHub API response")?;
    let json: serde_json::Value =
        serde_json::from_slice(&body).with_context(|| format!("parsing {api}"))?;
    let assets = json
        .get("assets")
        .and_then(|v| v.as_array())
        .with_context(|| format!("no `assets` array in {api}"))?;
    let url = assets
        .iter()
        .find_map(|asset| {
            let name = asset.get("name")?.as_str()?;
            if name.to_ascii_lowercase().ends_with(".zip") {
                asset
                    .get("browser_download_url")?
                    .as_str()
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .with_context(|| format!("no .zip asset in {repo}'s latest release"))?;

    let _ = std::fs::write(&cache_path, &url);
    save_meta(
        &stamp_path,
        &Meta {
            etag: None,
            fetched_at: now(),
            size: url.len() as u64,
        },
    );
    Ok(url)
}

/// Extract the first entry whose path (case-insensitive) matches any of
/// `candidates` from `zip_path` into `dest`. Overwrites any existing file
/// at `dest`. Fails loudly if no candidate is found — that catches
/// upstream release layouts shifting before they bite a user mid-write.
fn extract_named(zip_path: &Path, candidates: &[&str], dest: &Path) -> Result<()> {
    let f =
        std::fs::File::open(zip_path).with_context(|| format!("opening {}", zip_path.display()))?;
    let mut archive =
        zip::ZipArchive::new(f).with_context(|| format!("parsing zip {}", zip_path.display()))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("reading zip entry {i}"))?;
        let lower = entry.name().to_ascii_lowercase();
        if candidates.iter().any(|c| lower == c.to_ascii_lowercase()) {
            let mut out = std::fs::File::create(dest)
                .with_context(|| format!("creating {}", dest.display()))?;
            std::io::copy(&mut entry, &mut out)
                .with_context(|| format!("extracting {} → {}", entry.name(), dest.display()))?;
            return Ok(());
        }
    }
    anyhow::bail!(
        "none of {:?} found inside {}; upstream FreeDOS layout may have moved",
        candidates,
        zip_path.display()
    )
}
