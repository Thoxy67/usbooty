// Thin C++ shim exposing just enough of QTranslator for the Rust side to
// load and install a .qm file. Compiled by cxx-qt-build alongside the
// generated cxx bridge; included from src/translations.rs.

#pragma once

#include <QCoreApplication>
#include <QQmlApplicationEngine>
#include <QString>
#include <QTranslator>
#include <string>

inline QTranslator *translator_new() {
    return new QTranslator();
}

inline bool translator_load(QTranslator *tr, const std::string &path) {
    if (!tr) {
        return false;
    }
    // QTranslator::load accepts both filesystem paths and Qt resource paths
    // (":/i18n/..."), but only via QString — convert from std::string here.
    return tr->load(QString::fromStdString(path));
}

inline void translator_install(QTranslator *tr) {
    if (tr) {
        QCoreApplication::installTranslator(tr);
    }
}

inline void translator_remove(QTranslator *tr) {
    if (tr) {
        QCoreApplication::removeTranslator(tr);
    }
}

inline void translator_delete(QTranslator *tr) {
    delete tr;
}

// Force every QML `qsTr()` binding to re-evaluate. QCoreApplication does
// fire QEvent::LanguageChange when install/removeTranslator is called,
// but in practice QML's TranslationBindings only refresh reliably when
// QQmlEngine::retranslate() is invoked explicitly. The engine pointer
// comes from main.rs's `QQmlApplicationEngine` via cxx-qt-lib.
inline void engine_retranslate(QQmlApplicationEngine *engine) {
    if (engine) {
        engine->retranslate();
    }
}
