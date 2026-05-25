//! `usbooty` — the unprivileged Qt/QML front-end.
//!
//! This process never touches a block device itself. It enumerates devices,
//! analyzes ISOs, builds a [`Job`](usbooty_core::Job), and delegates the actual
//! write to the privileged helper (see [`runner`]).

// Disable the console window on Windows; harmless elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod bridge;
mod deps;
mod devices;
mod iso;
mod resources;
mod runner;
mod smart;
mod timezones;
mod windisco;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

fn main() {
    // Use the cross-platform Fusion style for a predictable desktop look,
    // rather than whatever Qt Quick Controls style the system defaults to.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Fusion");
    }

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

    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/com/usbooty/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
