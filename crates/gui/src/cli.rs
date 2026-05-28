//! Tiny command-line parser for the GUI entry point.
//!
//! Rufus exposes `-i ISO` and `-w SECONDS` for scripted runs; the analogous
//! flags here are `--iso PATH` and `--device PATH`. They preload the obvious
//! starting state so a user can drive usbooty from a desktop shortcut or a
//! shell command without clicking through the picker every time. The full
//! Rufus CLI surface (locale, filesystem, wait, gui-off) is intentionally
//! deferred to a future TODO entry — this is the minimum that makes the
//! decompression path testable headlessly.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Result of parsing `argv[1..]`.
pub enum Parsed {
    /// `--help` / `-h` was given; `main` should print and exit cleanly.
    Help,
    /// Anything else (possibly with no flags at all).
    Args(StartupArgs),
}

/// Resolved startup arguments handed to the QObject at engine load.
#[derive(Default, Clone)]
pub struct StartupArgs {
    pub iso: Option<PathBuf>,
    pub device: Option<PathBuf>,
}

/// Process-wide slot the QObject reads from `apply_cli`.
static ARGS: OnceLock<StartupArgs> = OnceLock::new();

/// Parse a flat slice of arguments (typically `&std::env::args()[1..]`).
///
/// Unknown flags are tolerated with a warning to stderr — the GUI must still
/// be usable when a desktop file passes something unexpected, and locking the
/// user out of the app over an argv typo would be a poor trade.
pub fn parse(args: &[String]) -> Parsed {
    let mut out = StartupArgs::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => return Parsed::Help,
            "-i" | "--iso" => {
                if let Some(v) = args.get(i + 1) {
                    out.iso = Some(PathBuf::from(v));
                    i += 2;
                    continue;
                }
                eprintln!("usbooty: --iso requires a path argument");
            }
            "--device" => {
                if let Some(v) = args.get(i + 1) {
                    out.device = Some(PathBuf::from(v));
                    i += 2;
                    continue;
                }
                eprintln!("usbooty: --device requires a path argument");
            }
            other => {
                eprintln!("usbooty: unrecognized argument: {other}");
            }
        }
        i += 1;
    }
    Parsed::Args(out)
}

/// Print usage to stdout in the style most CLIs use.
pub fn print_help() {
    println!(
        "usbooty: Bootable USB Creator\n\n\
         USAGE:\n  \
             usbooty [OPTIONS]\n\n\
         OPTIONS:\n  \
             -i, --iso <PATH>     Preload an ISO/IMG (or compressed image) at startup\n      \
                 --device <PATH>  Preselect a target block device (e.g. /dev/sdX)\n  \
             -h, --help           Print this help and exit"
    );
}

/// Store parsed args for the QObject to read on engine load.
pub fn install(args: StartupArgs) {
    let _ = ARGS.set(args);
}

/// Returns the stored args, if any. Called by the QObject from
/// `apply_startup_args` after the QML root component has finished loading.
pub fn take() -> Option<StartupArgs> {
    ARGS.get().cloned()
}
