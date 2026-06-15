//! Native port of Rufus's "Fido" logic for fetching official Windows ISOs.
//!
//! Microsoft does not publish stable ISO URLs: a link is obtained by querying
//! their `software-download-connector` API with a per-session GUID. Before the
//! API will answer, that GUID must clear two anti-bot ("Sentinel") gates,
//! exactly as Fido's PowerShell script does:
//!
//! 1. whitelist the GUID through `vlscppe.microsoft.com/tags`;
//! 2. complete an `ov-df.microsoft.com` challenge by fetching `mdt.js`,
//!    reading the `w` / `rticks` values it embeds, and echoing them
//!    straight back.
//!
//! Skipping step 2 (as older builds did) makes Microsoft reject the
//! `getskuinformationbyproductedition` call with a Type-9 Sentinel error.
//!
//! Those endpoints and the product-edition IDs are reverse-engineered and
//! change over time, so this module is isolated and fails gracefully; API
//! breakage never affects the core USB-writing paths. When Microsoft's
//! anti-bot still rejects the request (common on VPNs / flagged ISPs), the UI
//! falls back to opening the official download page in a browser.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A full browser User-Agent; Microsoft's anti-bot rejects obvious non-browsers.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";
/// The `profile` parameter Fido sends to the API.
const PROFILE: &str = "606624d44113";
/// The anti-bot `org_id` Fido uses when whitelisting a session.
const ORG_ID: &str = "y6jn8c31";
/// The (currently constant) `instanceId` for the `ov-df.microsoft.com` challenge.
const INSTANCE_ID: &str = "560dc9f3-1aa5-4a2f-b63c-9e18f8d0e175";
/// The public download page, loaded to seed cookies, and used as the Referer.
const DOWNLOAD_PAGE: &str = "https://www.microsoft.com/software-download/windows11";

/// Whole-call timeout for the small catalog / anti-bot requests. `ureq` 3
/// defaults to *no* timeout, so without this a stalled connection wedges the
/// app in its busy state forever.
const API_TIMEOUT: Duration = Duration::from_secs(30);

/// A selectable Windows release for the download dialog.
///
/// Microsoft exposes some variants as wholly separate "product editions"
/// rather than as architectures or SKUs of one edition. Two cases matter:
///   * ARM64 is its own product-edition ID (Microsoft never treated it as a
///     CPU arch the way it did x86/x64), so a release that offers ARM64 lists
///     *two* IDs that we query and merge.
///   * The China-specific images (government-mandated builds) are separate
///     product editions too, surfaced here as their own releases.
pub struct Release {
    /// Brand name shown in the release combo.
    pub name: &'static str,
    /// Microsoft product-edition IDs to query and merge. Multiple IDs cover
    /// architectures Microsoft models as separate products (x64 vs ARM64).
    /// Update these from the Fido script when Microsoft ships a new release.
    pub edition_ids: &'static [u32],
    /// Whether this is a Windows 10 release; selects the right Microsoft
    /// download page for the browser fallback.
    pub win10: bool,
}

/// Selectable Windows releases. The IDs mirror Fido's data table.
pub const RELEASES: &[Release] = &[
    Release {
        name: "Windows 11",
        edition_ids: &[3321, 3324],
        win10: false,
    },
    Release {
        name: "Windows 11 Home China",
        edition_ids: &[3322, 3325],
        win10: false,
    },
    Release {
        name: "Windows 11 Pro China",
        edition_ids: &[3323, 3326],
        win10: false,
    },
    Release {
        name: "Windows 10",
        edition_ids: &[2618],
        win10: true,
    },
    Release {
        name: "Windows 10 Home China",
        edition_ids: &[2378],
        win10: true,
    },
];

/// A UEFI Shell build downloadable from the `pbatard/UEFI-Shell` project.
/// Unlike the Windows images these are plain GitHub release assets, so the URL
/// is deterministic and no anti-bot dance is needed.
struct ShellBuild {
    /// EFI Shell version (also part of the asset name), e.g. "2.2".
    version: &'static str,
    /// Git tag / release, e.g. "26H1" (used in both the URL and the label).
    tag: &'static str,
    /// Human detail shown in the label, e.g. "edk2-stable202602".
    detail: &'static str,
    /// Whether a Debug asset exists alongside the Release one.
    has_debug: bool,
}

/// The selectable UEFI Shell builds, mirroring Fido's data table (newest
/// first). 2.2 ships both Release and Debug; the legacy 2.0 build is
/// Release-only.
const SHELL_BUILDS: &[ShellBuild] = &[
    ShellBuild {
        version: "2.2",
        tag: "26H1",
        detail: "edk2-stable202602",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "25H2",
        detail: "edk2-stable202511",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "25H1",
        detail: "edk2-stable202505",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "24H2",
        detail: "edk2-stable202411",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "24H1",
        detail: "edk2-stable202405",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "23H2",
        detail: "edk2-stable202311",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "23H1",
        detail: "edk2-stable202305",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "22H2",
        detail: "edk2-stable202211",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "22H1",
        detail: "edk2-stable202205",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "21H2",
        detail: "edk2-stable202108",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "21H1",
        detail: "edk2-stable202105",
        has_debug: true,
    },
    ShellBuild {
        version: "2.2",
        tag: "20H2",
        detail: "edk2-stable202011",
        has_debug: true,
    },
    ShellBuild {
        version: "2.0",
        tag: "4.632",
        detail: "20100426",
        has_debug: false,
    },
];

/// One concrete downloadable UEFI Shell ISO (a build plus a Release/Debug
/// variant), already resolved to a label and direct URL.
#[derive(Clone)]
pub struct ShellOption {
    /// Label shown in the combo.
    pub label: String,
    /// Direct GitHub download URL.
    pub url: String,
}

/// Flatten the build table into one entry per downloadable ISO (each build's
/// Release, plus its Debug when one exists).
pub fn uefi_shell_options() -> Vec<ShellOption> {
    let mut out = Vec::new();
    for b in SHELL_BUILDS {
        for (variant, suffix) in [("Release", "RELEASE"), ("Debug", "DEBUG")] {
            if suffix == "DEBUG" && !b.has_debug {
                continue;
            }
            out.push(ShellOption {
                label: format!(
                    "UEFI Shell {ver} {tag} ({detail}), {variant}",
                    ver = b.version,
                    tag = b.tag,
                    detail = b.detail,
                ),
                url: format!(
                    "https://github.com/pbatard/UEFI-Shell/releases/download/{tag}/\
                     UEFI-Shell-{ver}-{tag}-{suffix}.iso",
                    tag = b.tag,
                    ver = b.version,
                ),
            });
        }
    }
    out
}

/// One anti-bot-cleared HTTP session bound to a single product-edition ID.
/// The very same agent (cookie jar) and GUID that fetched a SKU's languages
/// must be reused to request that SKU's download links.
#[derive(Clone)]
struct Session {
    /// HTTP agent carrying the session cookies.
    agent: ureq::Agent,
    /// Anti-bot session GUID.
    session_id: String,
}

/// An opaque SKU id paired with the session that produced it.
#[derive(Clone)]
struct Sku {
    /// Index into [`Catalog::sessions`].
    session: usize,
    /// Opaque SKU identifier used to request the download links.
    id: String,
}

/// A language offered for a release. The same language may be backed by SKUs
/// from several product editions (e.g. an x64 edition and an ARM64 edition),
/// each carrying its own session; all are queried when listing downloads.
#[derive(Clone)]
pub struct Language {
    /// Localized language name shown to the user.
    pub display: String,
    /// English language key (e.g. "French"), used for locale matching.
    name: String,
    /// SKUs across every product edition that offers this language.
    skus: Vec<Sku>,
}

/// One concrete downloadable ISO (a specific architecture / variant).
#[derive(Clone)]
pub struct DownloadOption {
    /// Label shown to the user (the ISO file name).
    pub label: String,
    /// Direct, time-limited download URL.
    pub url: String,
}

/// The languages for one release, bound to the cookie-jar sessions that
/// fetched them.
#[derive(Clone)]
pub struct Catalog {
    /// One session per product edition that answered, referenced by `Sku`.
    sessions: Vec<Session>,
    /// Available languages, sorted by display name.
    pub languages: Vec<Language>,
}

/// Open and anti-bot-clear a fresh session, then return the parsed
/// `getskuinformationbyproductedition` response for one product edition.
fn open_session(edition_id: u32) -> Result<(Session, serde_json::Value)> {
    // A single agent with a cookie jar is reused for every request below.
    // Every call through it carries the catalog timeout; these are all small
    // JSON / page fetches.
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(API_TIMEOUT))
            .build(),
    );
    let session_id = new_guid();

    // Seed cookies by loading the public download page, like a browser would.
    let _ = agent
        .get(DOWNLOAD_PAGE)
        .header("User-Agent", USER_AGENT)
        .call()
        .and_then(|mut r| r.body_mut().read_to_vec());

    // Gate 1: whitelist the session with Microsoft's anti-bot endpoint.
    let _ = agent
        .get(format!(
            "https://vlscppe.microsoft.com/tags?org_id={ORG_ID}&session_id={session_id}"
        ))
        .header("User-Agent", USER_AGENT)
        .call();

    // Gate 2: complete the ov-df challenge for the same session GUID.
    clear_anti_bot(&agent, &session_id);

    let json = get_json(
        &agent,
        &format!(
            "https://www.microsoft.com/software-download-connector/api/\
             getskuinformationbyproductedition?profile={PROFILE}&productEditionId={edition_id}\
             &SKU=undefined&friendlyFileName=undefined&Locale=en-US&sessionID={session_id}"
        ),
    )
    .context("requesting the Windows language list")?;
    check_errors(&json)?;
    Ok((Session { agent, session_id }, json))
}

/// Fetch and merge the languages available across a release's product
/// editions. Each edition (e.g. x64 and ARM64) is queried in its own session;
/// SKUs are grouped by language so that listing a language later pulls every
/// architecture Microsoft offers for it.
pub fn fetch_languages(edition_ids: &[u32]) -> Result<Catalog> {
    let mut sessions = Vec::new();
    // First-seen order is preserved here, then sorted by display name below.
    let mut languages: Vec<Language> = Vec::new();
    let mut last_err = None;

    for &edition_id in edition_ids {
        let (session, json) = match open_session(edition_id) {
            Ok(pair) => pair,
            // One edition failing (e.g. an ARM64 ID Microsoft retired, or a
            // transient anti-bot rejection) must not sink the whole release:
            // keep whatever the other editions returned.
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };

        let skus = match json
            .get("Skus")
            .and_then(|v| v.as_array())
            .filter(|s| !s.is_empty())
        {
            Some(skus) => skus,
            None => continue,
        };

        let session_index = sessions.len();
        for sku in skus {
            let id = match sku.get("Id") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => continue,
            };
            let name = sku
                .get("Language")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let display = sku
                .get("LocalizedLanguage")
                .or_else(|| sku.get("Language"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let entry = Sku {
                session: session_index,
                id,
            };
            match languages
                .iter_mut()
                .find(|l| l.name == name && !name.is_empty())
            {
                Some(lang) => lang.skus.push(entry),
                None => languages.push(Language {
                    display,
                    name,
                    skus: vec![entry],
                }),
            }
        }
        sessions.push(session);
    }

    if languages.is_empty() {
        return Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("Microsoft returned no languages; the API may have changed")
        }));
    }
    languages.sort_by(|a, b| a.display.cmp(&b.display));
    Ok(Catalog {
        sessions,
        languages,
    })
}

impl Catalog {
    /// Language display names, parallel to `self.languages`.
    pub fn language_names(&self) -> Vec<String> {
        self.languages.iter().map(|l| l.display.clone()).collect()
    }

    /// Index of the language that best matches the host system locale, or 0
    /// when nothing matches. Used to pre-select the combo without committing
    /// the user to it (a port of Fido's `Select-Language`).
    pub fn default_language_index(&self) -> i32 {
        let locale = system_locale();
        self.languages
            .iter()
            .position(|l| language_matches(&locale, &l.name))
            .unwrap_or(0) as i32
    }

    /// Fetch the concrete download options (architectures) for one language,
    /// querying every product edition's SKU and merging the results.
    pub fn fetch_options(&self, language_index: usize) -> Result<Vec<DownloadOption>> {
        let language = self
            .languages
            .get(language_index)
            .context("no language selected")?;

        let mut out: Vec<DownloadOption> = Vec::new();
        let mut last_err = None;
        for sku in &language.skus {
            let Some(session) = self.sessions.get(sku.session) else {
                continue;
            };
            let body = session
                .agent
                .get(format!(
                    "https://www.microsoft.com/software-download-connector/api/\
                     GetProductDownloadLinksBySku?profile={PROFILE}&productEditionId=undefined\
                     &SKU={}&friendlyFileName=undefined&Locale=en-US&sessionID={}",
                    sku.id, session.session_id
                ))
                .header("User-Agent", USER_AGENT)
                // Microsoft's servers deny this request without a Referer.
                .header("Referer", DOWNLOAD_PAGE)
                .call()
                .and_then(|mut r| r.body_mut().read_to_vec())
                .context("requesting the Windows download links");
            let body = match body {
                Ok(b) => b,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };
            let json: serde_json::Value =
                serde_json::from_slice(&body).context("parsing the download-links response")?;
            if let Err(e) = check_errors(&json) {
                last_err = Some(e);
                continue;
            }

            let Some(options) = json
                .get("ProductDownloadOptions")
                .and_then(|v| v.as_array())
            else {
                continue;
            };
            for option in options {
                if let Some(url) = option.get("Uri").and_then(|u| u.as_str()) {
                    // Distinct product editions can echo the same arch; keep
                    // the listing unique by URL.
                    if out.iter().any(|o| o.url == url) {
                        continue;
                    }
                    out.push(DownloadOption {
                        label: file_name_from_url(url),
                        url: url.to_string(),
                    });
                }
            }
        }
        if out.is_empty() {
            return Err(last_err
                .unwrap_or_else(|| anyhow::anyhow!("Microsoft returned no download options")));
        }
        Ok(out)
    }
}

/// The host UI locale as a lowercased BCP-47-ish tag (e.g. `fr-ca`), derived
/// from the usual environment variables. Empty when none are set.
fn system_locale() -> String {
    let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    // Strip the ".UTF-8" / "@modifier" suffixes and normalise the separator.
    raw.split(['.', '@'])
        .next()
        .unwrap_or("")
        .replace('_', "-")
        .to_lowercase()
}

/// Whether the Microsoft English language name `name` is the best fit for the
/// host `locale` (both already lowercased; `locale` uses `-` separators). A
/// port of Fido's `Select-Language`, trimmed to the substring tests that
/// matter and with Fido's Bulgarian copy-paste bug fixed.
fn language_matches(locale: &str, name: &str) -> bool {
    let lang = locale.split('-').next().unwrap_or(locale);
    let has = |s: &str| name.contains(s);
    // Region-specific variants first, so they win over the bare-language rules.
    match locale {
        "en-us" => return name == "english",
        "pt-br" => return has("brazil"),
        "pt-pt" => return name == "portuguese",
        "fr-ca" => return has("french") && has("canad"),
        "zh-cn" => return has("chinese") && has("simp"),
        "zh-tw" => return has("chinese") && has("trad"),
        "es-es" => return name == "spanish",
        _ => {}
    }
    match lang {
        "ar" => has("arabic"),
        "bg" => has("bulgar"),
        "zh" => has("chinese"),
        "hr" => has("croat"),
        "cs" | "cz" => has("czech"),
        "da" => has("danish"),
        "nl" => has("dutch"),
        "en" => has("english") && (has("inter") || has("ingdom")),
        "et" => has("eston"),
        "fi" => has("finn"),
        "fr" => name == "french",
        "de" => has("german"),
        "el" => has("greek"),
        "he" => has("hebrew"),
        "hu" => has("hungar"),
        "id" => has("indones"),
        "it" => has("italia"),
        "ja" => has("japan"),
        "ko" => has("korea"),
        "lv" => has("latvia"),
        "lt" => has("lithuania"),
        "ms" => has("malay"),
        "nb" | "nn" | "no" => has("norw"),
        "fa" => has("persia"),
        "pl" => has("polish"),
        "pt" => name == "portuguese",
        "ro" => has("romania"),
        "ru" => has("russia"),
        "sr" => has("serbia"),
        "sk" => has("slovak"),
        "sl" => has("slovenia"),
        "es" => has("spanish"),
        "sv" => has("swed"),
        "th" => has("thai"),
        "tr" => has("turk"),
        "uk" => has("ukrain"),
        "vi" => has("vietnam"),
        _ => false,
    }
}

/// Probe the size of a download via a HEAD request, so the UI can show it and
/// guard against a destination that lacks the room. `None` when the server
/// withholds `Content-Length` (the streaming download still works).
pub fn content_length(url: &str) -> Option<u64> {
    let resp = ureq::head(url)
        .config()
        .timeout_global(Some(API_TIMEOUT))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .ok()?;
    resp.headers()
        .get("content-length")?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

/// Download `url` into `dest_dir`, returning `(saved path, every digest)`.
/// Each digest is computed from the bytes as they stream past, free and ready
/// the instant the download finishes, so no re-read of the ISO is needed.
/// `progress` is called with `(downloaded, total)` and is throttled internally.
///
/// The stream lands in a hidden `.part` temp file that is renamed into place
/// only on success (uniquified if the name is taken), so an existing ISO of
/// the same name is never truncated, and a failed or cancelled download
/// never leaves a plausible-looking partial file behind.
pub fn download(
    url: &str,
    dest_dir: &Path,
    abort: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<(PathBuf, crate::iso::IsoHashes)> {
    use md5::Md5;
    use sha1::Sha1;
    use sha2::{Digest, Sha256, Sha512};

    // Connect/first-byte timeouts only: the body itself legitimately takes
    // however long a multi-GB transfer takes on the user's link, so it gets
    // no global deadline (cancellation covers a user-visible stall).
    let mut response = ureq::get(url)
        .config()
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .context("starting the download")?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let final_name = file_name_from_url(url);
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{final_name}."))
        .suffix(".part")
        .tempfile_in(dest_dir)
        .with_context(|| format!("creating a download temp file in {}", dest_dir.display()))?;
    let out = tmp.as_file_mut();

    let mut reader = response.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last = Instant::now();
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut blake3 = blake3::Hasher::new();
    let stream = (|| -> Result<()> {
        loop {
            if abort.load(Ordering::SeqCst) {
                bail!("download cancelled");
            }
            let n = reader
                .read(&mut buf)
                .context("reading the download stream")?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).context("writing the ISO")?;
            let chunk = &buf[..n];
            md5.update(chunk);
            sha1.update(chunk);
            sha256.update(chunk);
            sha512.update(chunk);
            blake3.update(chunk);
            done += n as u64;
            if last.elapsed() >= Duration::from_millis(150) {
                progress(done, total);
                last = Instant::now();
            }
        }
        // A server that closes the stream early would otherwise hand the
        // user a truncated ISO that looks complete by name.
        if total > 0 && done < total {
            bail!("the connection closed after {done} of {total} bytes");
        }
        Ok(())
    })();
    // Cancelled or failed mid-stream: dropping `tmp` deletes the `.part`
    // file, so nothing truncated is left looking like a complete ISO.
    stream?;
    progress(done, total.max(done));

    // Move the finished download into place under a name that does not
    // clobber anything already there.
    let dest = unique_dest(dest_dir, &final_name);
    tmp.persist(&dest)
        .with_context(|| format!("moving the download to {}", dest.display()))?;

    Ok((
        dest,
        crate::iso::IsoHashes {
            md5: hex(&md5.finalize()),
            sha1: hex(&sha1.finalize()),
            sha256: hex(&sha256.finalize()),
            sha512: hex(&sha512.finalize()),
            blake3: blake3.finalize().to_hex().to_string(),
        },
    ))
}

/// Lowercase hex encoding of a fixed-size digest.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Turn a Microsoft `Errors` array into a helpful Rust error.
fn check_errors(json: &serde_json::Value) -> Result<()> {
    let Some(first) = json
        .get("Errors")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
    else {
        return Ok(());
    };
    // Type 9 is the anti-bot ("Sentinel") rejection.
    if first.get("Type").and_then(|v| v.as_i64()) == Some(9) {
        bail!(
            "Microsoft's anti-bot system rejected the request. This is common on \
             VPNs, datacenter connections, or some ISPs. Use the \"Open Microsoft \
             download page\" button below to download the ISO in your browser."
        );
    }
    let message = first
        .get("Value")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown error");
    bail!("Microsoft returned an error: {message}");
}

/// Complete Microsoft's `ov-df.microsoft.com` anti-bot challenge for `session_id`.
///
/// `mdt.js` embeds a `w` token and an `rticks` timestamp; the session is only
/// cleared once both are echoed back to the root endpoint. Best-effort: any
/// failure here just leaves the later API call to return the Sentinel error,
/// which is already surfaced gracefully by `check_errors`.
fn clear_anti_bot(agent: &ureq::Agent, session_id: &str) {
    let mdt = agent
        .get(format!(
            "https://ov-df.microsoft.com/mdt.js\
             ?instanceId={INSTANCE_ID}&PageId=si&session_id={session_id}"
        ))
        .header("User-Agent", USER_AGENT)
        .call()
        .and_then(|mut r| r.body_mut().read_to_vec());
    let mdt = match mdt {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => return,
    };

    // The script carries `...&w=<hex>"` and `...&rticks="+<digits>;`.
    let Some(w) = extract_token(&mdt, "&w=", |c| c.is_ascii_hexdigit()) else {
        return;
    };
    let Some(rticks) = extract_token(&mdt, "rticks=\"", |c| c.is_ascii_digit()) else {
        return;
    };
    let mdt_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let _ = agent
        .get(format!(
            "https://ov-df.microsoft.com/?session_id={session_id}&CustomerId={INSTANCE_ID}\
             &PageId=si&w={w}&mdt={mdt_ms}&rticks={rticks}"
        ))
        .header("User-Agent", USER_AGENT)
        .call();
}

/// Extract the run of `accept`-matching chars that follows `marker` in `text`,
/// skipping one optional leading `+` (the `rticks` value is written `"+1234`).
fn extract_token(text: &str, marker: &str, accept: impl Fn(char) -> bool) -> Option<String> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let rest = rest.strip_prefix('+').unwrap_or(rest);
    let token: String = rest.chars().take_while(|c| accept(*c)).collect();
    (!token.is_empty()).then_some(token)
}

/// GET a URL through `agent` and parse the body as JSON.
fn get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value> {
    let bytes = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Referer", DOWNLOAD_PAGE)
        .call()
        .and_then(|mut r| r.body_mut().read_to_vec())
        .context("HTTP request failed")?;
    serde_json::from_slice(&bytes).context("response was not valid JSON")
}

/// A path in `dir` for `name` that does not collide with an existing file:
/// the name itself when free, otherwise `name (1).iso`, `name (2).iso`, ...
/// (the counter lands before the extension). A prior good download is never
/// overwritten by a re-download of the same release.
fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, Some(e)),
        _ => (name, None),
    };
    for n in 1u32.. {
        let candidate = match ext {
            Some(ext) => dir.join(format!("{stem} ({n}).{ext}")),
            None => dir.join(format!("{stem} ({n})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 exhausted while uniquifying a file name")
}

/// Derive a `.iso` file name from a download URL.
fn file_name_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .and_then(|s| s.split('?').next())
        .filter(|s| s.to_lowercase().ends_with(".iso"))
        .map(str::to_owned)
        .unwrap_or_else(|| "Windows.iso".to_string())
}

/// Generate a random RFC-4122 v4 GUID string.
fn new_guid() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b[6] = (b[6] & 0x0F) | 0x40; // version 4
    b[8] = (b[8] & 0x3F) | 0x80; // variant 1
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hex_encodes_bytes_lowercase() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0x1a]), "000fff1a");
    }

    #[test]
    fn file_name_from_url_extracts_iso() {
        let url =
            "https://software.download.prss.microsoft.com/foo/Win11_24H2_English_x64.iso?t=abc";
        assert_eq!(file_name_from_url(url), "Win11_24H2_English_x64.iso");
    }

    #[test]
    fn file_name_from_url_falls_back_when_no_iso() {
        assert_eq!(
            file_name_from_url("https://example.com/redirect"),
            "Windows.iso"
        );
        assert_eq!(file_name_from_url(""), "Windows.iso");
    }

    #[test]
    fn extract_token_reads_run_of_matching_chars() {
        let body = r#"...&w=deadbeef" ... rticks="+1234567;..."#;
        assert_eq!(
            extract_token(body, "&w=", |c| c.is_ascii_hexdigit()),
            Some("deadbeef".to_string())
        );
        // The `+` prefix is silently skipped (this is what mdt.js emits).
        assert_eq!(
            extract_token(body, "rticks=\"", |c| c.is_ascii_digit()),
            Some("1234567".to_string())
        );
    }

    #[test]
    fn extract_token_returns_none_when_marker_missing() {
        assert_eq!(
            extract_token("nothing here", "&w=", |c| c.is_ascii_hexdigit()),
            None
        );
    }

    #[test]
    fn check_errors_passes_when_array_empty_or_absent() {
        check_errors(&json!({})).expect("no errors -> Ok");
        check_errors(&json!({ "Errors": [] })).expect("empty array -> Ok");
    }

    #[test]
    fn check_errors_flags_sentinel_type_9() {
        let v = json!({ "Errors": [ { "Type": 9, "Value": "blocked" } ] });
        let err = check_errors(&v).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("anti-bot"),
            "expected anti-bot hint, got: {msg}"
        );
    }

    #[test]
    fn check_errors_relays_generic_message() {
        let v = json!({ "Errors": [ { "Type": 1, "Value": "Something broke" } ] });
        let err = check_errors(&v).unwrap_err();
        assert!(format!("{err:#}").contains("Something broke"));
    }

    #[test]
    fn unique_dest_does_not_clobber_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            unique_dest(dir.path(), "Win11.iso"),
            dir.path().join("Win11.iso")
        );
        std::fs::write(dir.path().join("Win11.iso"), b"x").unwrap();
        assert_eq!(
            unique_dest(dir.path(), "Win11.iso"),
            dir.path().join("Win11 (1).iso")
        );
        std::fs::write(dir.path().join("Win11 (1).iso"), b"x").unwrap();
        assert_eq!(
            unique_dest(dir.path(), "Win11.iso"),
            dir.path().join("Win11 (2).iso")
        );
    }

    #[test]
    fn language_matches_handles_regions_and_bare_languages() {
        // Region-specific variants beat the bare-language rules.
        assert!(language_matches("pt-br", "portuguese (brazil)"));
        assert!(!language_matches("pt-br", "portuguese"));
        assert!(language_matches("pt-pt", "portuguese"));
        assert!(language_matches("fr-ca", "french canadian"));
        assert!(!language_matches("fr-ca", "french"));
        assert!(language_matches("zh-cn", "chinese (simplified)"));
        assert!(language_matches("zh-tw", "chinese (traditional)"));
        // Bare language prefixes.
        assert!(language_matches("fr-fr", "french"));
        assert!(language_matches("de-de", "german"));
        assert!(language_matches("en-us", "english"));
        assert!(language_matches("en-gb", "english international"));
        assert!(!language_matches("en-us", "english international"));
        // Fido's Bulgarian copy-paste bug is fixed here.
        assert!(language_matches("bg-bg", "bulgarian"));
        // No match falls through to false (caller defaults to index 0).
        assert!(!language_matches("xx-yy", "klingon"));
    }

    #[test]
    fn new_guid_has_v4_layout() {
        let g = new_guid();
        assert_eq!(g.len(), 36);
        // RFC 4122 v4 fixes the 15th char (version) and the 20th char (variant).
        assert_eq!(g.chars().nth(14), Some('4'));
        assert!(matches!(g.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
    }
}
