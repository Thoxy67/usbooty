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
//! service — the SHA-1 itself is the source of truth, the lookup just
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
/// into a verdict — never propagates network errors to the caller.
///
/// Runs on a worker thread; the call is HTTP-blocking with a short timeout
/// so a slow / unreachable service doesn't stall the hash-display path.
pub fn lookup(sha1_hex: &str) -> Option<AdguardVerdict> {
    if sha1_hex.len() != 40 || !sha1_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let url = format!("https://sha1.rg-adguard.net/api/{sha1_hex}");
    let resp = ureq::get(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .header("User-Agent", concat!("usbooty/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/json, text/plain;q=0.5")
        .call()
        .ok()?;

    if resp.status() != 200 {
        return None;
    }
    let body = resp.into_body().read_to_string().ok()?;
    parse_response(&body)
}

/// Best-effort parser for whatever shape the upstream service hands back.
///
/// The historical rg-adguard endpoint returned JSON, but the project has
/// been through several rewrites and currently sometimes returns minimal
/// HTML. Try JSON first; if that fails, scrape any filename + category
/// pair out of plain text. Any failure path is a silent `None`.
fn parse_response(body: &str) -> Option<AdguardVerdict> {
    // Path A: structured JSON.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let filename = json
            .get("name")
            .or_else(|| json.get("filename"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let category = json
            .get("category")
            .or_else(|| json.get("description"))
            .or_else(|| json.get("info"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !filename.is_empty() || !category.is_empty() {
            return Some(AdguardVerdict { filename, category });
        }
    }
    // Path B: text scrape — find a Microsoft-y filename or category hint.
    // Conservative; only emit a verdict when something definitely matches.
    let needle = body
        .lines()
        .find(|l| {
            let l = l.to_ascii_lowercase();
            l.contains(".iso") && (l.contains("win") || l.contains("microsoft"))
        })
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() < 200);
    needle.map(|line| AdguardVerdict {
        filename: line.to_string(),
        category: String::from("Verified upstream"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_with_filename() {
        let v = parse_response(
            r#"{"name":"Win11_24H2_English_x64.iso","category":"Windows 11"}"#,
        )
        .unwrap();
        assert_eq!(v.filename, "Win11_24H2_English_x64.iso");
        assert_eq!(v.category, "Windows 11");
        assert!(v.badge().contains("Windows 11"));
    }

    #[test]
    fn parses_text_fallback() {
        let v = parse_response("Filename: Win11_x64.iso (Windows 11)").unwrap();
        assert!(v.filename.contains("Win11_x64.iso"));
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
