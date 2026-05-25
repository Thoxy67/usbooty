// C++ implementation of the QTranslator shim declared in
// translator_bridge.h. Compiled by build.rs via the cc crate; linked into
// the GUI binary as a static lib.

#include "translator_bridge.h"

extern "C" {

QTranslator *usbooty_translator_new() {
    return new QTranslator();
}

bool usbooty_translator_load(QTranslator *tr, const char *path) {
    if (!tr || !path) {
        return false;
    }
    // QTranslator::load accepts both filesystem paths and Qt resource paths
    // ("qrc:/i18n/..." or ":/i18n/..."), but only via QString — convert here.
    return tr->load(QString::fromUtf8(path));
}

void usbooty_translator_install(QTranslator *tr) {
    if (tr) {
        QCoreApplication::installTranslator(tr);
    }
}

void usbooty_translator_remove(QTranslator *tr) {
    if (tr) {
        QCoreApplication::removeTranslator(tr);
    }
}

void usbooty_translator_delete(QTranslator *tr) {
    delete tr;
}

void usbooty_engine_retranslate(QQmlApplicationEngine *engine) {
    if (engine) {
        engine->retranslate();
    }
}

} // extern "C"
