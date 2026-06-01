//! User-preference persistence between runs.
//!
//! Stores a tiny JSON document at `~/.config/usbooty/settings.json`. Today
//! it only carries the *Force English* toggle, but the shape is
//! intentionally extensible: anything that wants to survive a restart
//! (last-used filesystem, last-used write method, beta channel flag, …)
//! drops a new field on the struct here.
//!
//! Loading is best-effort: a missing file, a syntax error, or a partial
//! schema all fall back to the defaults. The app should always start with
//! a usable state regardless of the disk's mood.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Everything usbooty remembers across launches.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Whether the GUI is forced into English regardless of the system
    /// locale. Mostly used by non-English speakers when filing bug
    /// reports / screenshots, so the strings match what English-locale
    /// users see.
    #[serde(default)]
    pub force_english: bool,
    /// Force the activity-log column to stay visible even when the
    /// buffer is empty. The default behaviour expands the panel on the
    /// first log line and collapses it again when the user hits Clear;
    /// power users who tail every write prefer "always there".
    #[serde(default)]
    pub show_logs_always: bool,
    /// Name every copied file in the activity log, not just the large ones.
    /// Forwarded into each job's options so the helper lifts its per-file
    /// size threshold; off keeps the default (only files past a few MiB).
    #[serde(default)]
    pub log_all_files: bool,
}

impl Settings {
    /// Load `settings.json` from the user's config dir. Any failure (file
    /// missing, parse error, IO error) yields a default Settings; start-up
    /// must never be blocked by a corrupt preferences file.
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Best-effort persist. Returns the I/O error to the caller for logging
    /// but never panics.
    pub fn save(&self) -> Result<()> {
        let path = settings_path().context("could not locate the user config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("serialising settings to JSON")?;
        std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

/// `~/.config/usbooty/settings.json`: `directories::ProjectDirs` picks the
/// right per-OS path. Returns `None` only on hosts so unusual we can't
/// figure out a config location at all.
fn settings_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("org", "usbooty", "usbooty")?;
    Some(dirs.config_dir().join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let s = Settings::default();
        assert!(!s.force_english);
    }

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
            force_english: true,
            show_logs_always: true,
            log_all_files: true,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert!(back.force_english);
        assert!(back.show_logs_always);
        assert!(back.log_all_files);
    }

    #[test]
    fn ignores_unknown_extra_fields() {
        // Old config files that still carry a `theme` field (from the
        // dropped dark-mode toggle) must not break parsing.
        let with_extra = r#"{"force_english": true, "theme": "dark"}"#;
        let parsed: Settings = serde_json::from_str(with_extra).unwrap();
        assert!(parsed.force_english);
    }
}
