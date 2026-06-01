//! Cross-check an ISO's SHA-1 against the community
//! [`sha1.rg-adguard.net`](https://sha1.rg-adguard.net) database.
//!
//! Rufus uses the same service to badge a Windows ISO as "this is a genuine
//! Microsoft retail image with the SHA-1 you'd expect". usbooty does the
//! same here, on demand, after `compute_hashes` runs.
//!
//! The lookup is fire-and-forget by design: any failure (network down, API
//! returns 4xx, response doesn't parse, etc.) just returns `None` and the
//! UI shows no badge. Never block a write on a verdict from a third-party
//! service; the SHA-1 itself is the source of truth, the lookup just
//! tells the user what *name* that SHA-1 is associated with upstream.

use std::time::Duration;

/// The summary string surfaced to the UI when an ISO's SHA-1 is recognised.
///
/// Empty / no entry → nothing to show; the QML side just hides the badge.
#[derive(Clone, Debug, Default)]
pub struct AdguardVerdict {
    /// Canonical filename the upstream catalog associates with this SHA-1.
    pub filename: String,
    /// Free-text description (e.g. "Windows 11" / "Windows 10 IoT LTSC").
    pub category: String,
}

impl AdguardVerdict {
    /// One-line label for the green "verified" badge. Empty when the
    /// verdict carries no useful name (treated as "no match" by the UI).
    pub fn badge(&self) -> String {
        match (self.filename.is_empty(), self.category.is_empty()) {
            (true, true) => String::new(),
            (false, true) => self.filename.clone(),
            (true, false) => self.category.clone(),
            (false, false) => format!("{} · {}", self.category, self.filename),
        }
    }
}

/// Query the upstream service for a SHA-1 hex string (40 lowercase hex
/// chars). Returns `None` when the response can't be confidently parsed
/// into a verdict, never propagates network errors to the caller.
///
/// Runs on a worker thread; the call is HTTP-blocking with a short timeout
/// so a slow / unreachable service doesn't stall the hash-display path.
pub fn lookup(sha1_hex: &str) -> Option<AdguardVerdict> {
    if sha1_hex.len() != 40 || !sha1_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    // The lookup is a plain GET search form: `search.php?sha1=<hex>&lang=...`.
    // (The older `/api/<hex>` path now 404s, which silently disabled the
    // badge.) `lang` only changes the page's UI text; the result table is the
    // same regardless.
    let url = format!("https://sha1.rg-adguard.net/search.php?sha1={sha1_hex}&lang=en-us");
    let resp = ureq::get(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .header("User-Agent", concat!("usbooty/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "text/html")
        .call()
        .ok()?;

    if resp.status() != 200 {
        return None;
    }
    let body = resp.into_body().read_to_string().ok()?;
    parse_response(&body)
}

/// Scrape the first matching file name out of the `search.php` results page.
///
/// On a hit, the page lists every match as an `<a>` linking into
/// `files.rg-adguard.net/file/<id>`; the anchor text is the file name, and the
/// first row is the canonical (Microsoft-named) image. On a miss the page says
/// "not found" and carries no such link, so we return `None`. Only the file
/// name is surfaced, so the badge reads e.g.
/// "Verified by rg-adguard: fr-fr_windows_11_..._x64_dvd_a1cf6c36.iso".
fn parse_response(body: &str) -> Option<AdguardVerdict> {
    const MARKER: &str = "files.rg-adguard.net/file/";
    let start = body.find(MARKER)?;
    // Skip past the rest of the opening `<a ...>` tag (the first `>` after the
    // href), then take the anchor text up to the closing `</a>`.
    let gt = body[start..].find('>')?;
    let name = body[start + gt + 1..].split("</a>").next()?.trim();
    if name.is_empty() || name.len() >= 200 {
        return None;
    }
    Some(AdguardVerdict {
        filename: name.to_string(),
        category: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of a real `search.php` hit: each match is an <a> into
    // files.rg-adguard.net, the first row being the canonical Microsoft name.
    const MATCH_HTML: &str = concat!(
        "<font size=\"5\">Match for this amount found in the database!</font>",
        "<table><thead><tr><th>File:</th></tr></thead>",
        "<tr><td><a href=\"https://files.rg-adguard.net/file/aa34423b\" target=\"_blank\">",
        "fr-fr_windows_11_x64_dvd.iso</a></td></tr>",
        "<tr><td><a href=\"https://files.rg-adguard.net/file/aa34423b\" target=\"_blank\">",
        "Win11_25H2_French_x64_v2.iso</a></td></tr></table>"
    );

    #[test]
    fn extracts_first_filename_from_match_page() {
        let v = parse_response(MATCH_HTML).unwrap();
        assert_eq!(v.filename, "fr-fr_windows_11_x64_dvd.iso");
        assert!(v.badge().contains("fr-fr_windows_11_x64_dvd.iso"));
    }

    #[test]
    fn no_match_page_returns_none() {
        // The miss page carries no files.rg-adguard.net link.
        assert!(parse_response("<p>SHA-1 ... not found in the database.</p>").is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_response("").is_none());
        assert!(parse_response("404 Not Found").is_none());
        assert!(parse_response("{}").is_none());
    }

    #[test]
    fn rejects_invalid_sha1() {
        // Wrong length / non-hex must short-circuit before any HTTP call.
        assert!(lookup("not-a-hash").is_none());
        assert!(lookup("a".repeat(39).as_str()).is_none());
    }
}
