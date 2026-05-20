use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("com.usbooty").qml_file("qml/main.qml"))
        .file("src/bridge.rs")
        .qt_module("Qml")
        .qt_module("Quick")
        // Bundle the app icon into the binary as `qrc:/icons/usbooty.svg`, so
        // QML can reference it whether running from the dev tree or installed.
        .qrc("qrc/icons.qrc")
        .build();
}
