//! `usbooty` — the unprivileged Qt/QML front-end.
//!
//! This process never touches a block device itself. It enumerates devices,
//! analyzes ISOs, builds a [`Job`](usbooty_core::Job), and delegates the actual
//! write to the privileged helper (see [`runner`]).

// Disable the console window on Windows; harmless elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use usbooty_gui::{cli, decompress, resources, translations};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match cli::parse(&args[1..]) {
        cli::Parsed::Help => {
            cli::print_help();
            return;
        }
        cli::Parsed::Args(parsed) => cli::install(parsed),
    }

    // Drop stale entries from the decompression cache. A user who works
    // with the same big `.iso.xz` daily keeps it warm; a one-off pick is
    // forgotten after 30 days so the cache doesn't grow unbounded.
    decompress::prune_cache(30);

    // Fetch the live UEFI Forum DBX revocation update in the background,
    // so the next ISO scan benefits from the freshest list. Soft-fails:
    // when the network is down or the URL moves, the SBAT scan keeps
    // working off the baked-in baseline.
    std::thread::spawn(resources::prime_revocation_db);

    let mut app = QGuiApplication::new();

    // Wayland decides which icon to show in the taskbar / titlebar by matching
    // the Wayland app-id (xdg-toplevel) against an installed `.desktop` file.
    // Setting the desktop file name here tells Qt to advertise that app-id, so
    // compositors find `org.usbooty.Usbooty.desktop` and use its Icon= entry.
    // Without this, Wayland falls back to a generic icon — even though the
    // `ApplicationWindow.icon` set in QML is honoured by X11 and as a backup.
    QGuiApplication::set_desktop_file_name(&QString::from("org.usbooty.Usbooty"));
    if let Some(app) = app.as_mut() {
        app.set_application_name(&QString::from("usbooty"));
    }

    // Load persisted settings. `set_force_english` handles both branches
    // — install the system .qm when false, leave the English baseline
    // alone when true — so it's the only translation call we need here.
    let prefs = usbooty_gui::settings::Settings::load();
    translations::set_force_english(prefs.force_english);

    let mut engine = QQmlApplicationEngine::new();

    // Hand the translation module a pointer to the engine so the runtime
    // language toggle can force a `retranslate()` after swapping
    // translators. Done before `load()` so the engine is fully constructed.
    if let Some(mut engine_pin) = engine.as_mut() {
        // SAFETY: the pinned reference is borrowed only to grab a raw
        // pointer the C++ side stores; the engine outlives the call.
        let raw: *mut _ = unsafe { engine_pin.as_mut().get_unchecked_mut() };
        translations::register_engine(raw as *mut std::ffi::c_void);
        engine_pin.load(&QUrl::from("qrc:/qt/qml/com/usbooty/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
