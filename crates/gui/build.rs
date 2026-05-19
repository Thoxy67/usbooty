use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("com.usbooty").qml_file("qml/main.qml"))
        .file("src/bridge.rs")
        .qt_module("Qml")
        .qt_module("Quick")
        .build();
}
