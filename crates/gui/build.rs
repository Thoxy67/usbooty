use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("com.usbooty").qml_file("qml/main.qml"))
        .file("src/bridge.rs")
        // QTranslator shim — wraps three QCoreApplication calls so the
        // Rust side can load .qm files and install them at startup.
        .file("src/translations.rs")
        .include_dir("include")
        .qt_module("Qml")
        .qt_module("Quick")
        // Bundle the app icon into the binary as `qrc:/icons/usbooty.svg`, so
        // QML can reference it whether running from the dev tree or installed.
        .qrc("qrc/icons.qrc")
        // Compiled translations (`.qm`) embedded as qrc:/i18n/usbooty_<loc>.qm
        // so the binary can find them whether running from the dev tree or
        // installed system-wide.
        .qrc("qrc/translations.qrc")
        .build();
}
