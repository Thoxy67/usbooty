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
mod windisco;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    // Use the cross-platform Fusion style for a predictable desktop look,
    // rather than whatever Qt Quick Controls style the system defaults to.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Fusion");
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/com/usbooty/qml/main.qml"));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
