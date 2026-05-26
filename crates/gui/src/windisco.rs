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
//! change over time, so this module is isolated and fails gracefully — API
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
/// The public download page — loaded to seed cookies, and used as the Referer.
const DOWNLOAD_PAGE: &str = "https://www.microsoft.com/software-download/windows11";

/// Selectable Windows releases: `(display name, Microsoft product-edition ID)`.
/// Update the IDs from the Fido script when Microsoft ships a new release.
pub const RELEASES: &[(&str, u32)] = &[("Windows 11", 3321), ("Windows 10", 2618)];

/// A language offered for a release, paired with its opaque SKU id.
#[derive(Clone)]
pub struct Language {
    /// Localized language name shown to the user.
    pub display: String,
    /// Opaque SKU identifier used to request the download links.
    sku_id: String,
}

/// One concrete downloadable ISO (a specific architecture / variant).
#[derive(Clone)]
pub struct DownloadOption {
    /// Label shown to the user (the ISO file name).
    pub label: String,
    /// Direct, time-limited download URL.
    pub url: String,
}

/// The languages for one release, bound to the cookie-jar session that fetched
/// them — the very same `Agent` must be reused to request download links.
#[derive(Clone)]
pub struct Catalog {
    /// HTTP agent carrying the session cookies.
    agent: ureq::Agent,
    /// Anti-bot session GUID.
    session_id: String,
    /// Available languages, sorted by display name.
    pub languages: Vec<Language>,
}

/// Fetch the list of languages available for a Windows product edition.
pub fn fetch_languages(edition_id: u32) -> Result<Catalog> {
    // A single agent with a cookie jar is reused for every request below.
    let agent = ureq::Agent::new_with_defaults();
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

    let skus = json
        .get("Skus")
        .and_then(|v| v.as_array())
        .filter(|s| !s.is_empty())
        .context("Microsoft returned no languages — the API may have changed")?;

    let mut languages = Vec::new();
    for sku in skus {
        let sku_id = match sku.get("Id") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => continue,
        };
        let display = sku
            .get("LocalizedLanguage")
            .or_else(|| sku.get("Language"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        languages.push(Language { display, sku_id });
    }
    languages.sort_by(|a, b| a.display.cmp(&b.display));
    Ok(Catalog {
        agent,
        session_id,
        languages,
    })
}

impl Catalog {
    /// Language display names, parallel to `self.languages`.
    pub fn language_names(&self) -> Vec<String> {
        self.languages.iter().map(|l| l.display.clone()).collect()
    }

    /// Fetch the concrete download options (architectures) for one language.
    pub fn fetch_options(&self, language_index: usize) -> Result<Vec<DownloadOption>> {
        let language = self
            .languages
            .get(language_index)
            .context("no language selected")?;

        let body = self
            .agent
            .get(format!(
                "https://www.microsoft.com/software-download-connector/api/\
                 GetProductDownloadLinksBySku?profile={PROFILE}&productEditionId=undefined\
                 &SKU={}&friendlyFileName=undefined&Locale=en-US&sessionID={}",
                language.sku_id, self.session_id
            ))
            .header("User-Agent", USER_AGENT)
            // Microsoft's servers deny this request without a Referer.
            .header("Referer", DOWNLOAD_PAGE)
            .call()
            .and_then(|mut r| r.body_mut().read_to_vec())
            .context("requesting the Windows download links")?;
        let json: serde_json::Value =
            serde_json::from_slice(&body).context("parsing the download-links response")?;
        check_errors(&json)?;

        let options = json
            .get("ProductDownloadOptions")
            .and_then(|v| v.as_array())
            .filter(|o| !o.is_empty())
            .context("Microsoft returned no download options")?;

        let mut out = Vec::new();
        for option in options {
            if let Some(url) = option.get("Uri").and_then(|u| u.as_str()) {
                out.push(DownloadOption {
                    label: file_name_from_url(url),
                    url: url.to_string(),
                });
            }
        }
        if out.is_empty() {
            bail!("no ISO links were present in Microsoft's response");
        }
        Ok(out)
    }
}

/// Download `url` into `dest_dir`, returning `(saved path, every digest)`.
/// Each digest is computed from the bytes as they stream past, free and ready
/// the instant the download finishes, so no re-read of the ISO is needed.
/// `progress` is called with `(downloaded, total)` and is throttled internally.
pub fn download(
    url: &str,
    dest_dir: &Path,
    abort: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<(PathBuf, crate::iso::IsoHashes)> {
    use md5::Md5;
    use sha1::Sha1;
    use sha2::{Digest, Sha256, Sha512};

    let mut response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .context("starting the download")?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let dest = dest_dir.join(file_name_from_url(url));
    let mut out = File::create(&dest).with_context(|| format!("creating {}", dest.display()))?;

    let mut reader = response.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last = Instant::now();
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut blake3 = blake3::Hasher::new();
    loop {
        if abort.load(Ordering::SeqCst) {
            let _ = std::fs::remove_file(&dest);
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
    progress(done, total.max(done));

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
             VPNs, datacenter connections, or some ISPs — use the \"Open Microsoft \
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
    fn new_guid_has_v4_layout() {
        let g = new_guid();
        assert_eq!(g.len(), 36);
        // RFC 4122 v4 fixes the 15th char (version) and the 20th char (variant).
        assert_eq!(g.chars().nth(14), Some('4'));
        assert!(matches!(g.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
    }
}
