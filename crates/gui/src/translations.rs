//! Locale-aware translation loading via `QTranslator`.
//!
//! cxx-qt-lib doesn't expose `QTranslator` itself, so we hand-roll a tiny
//! `extern "C++"` block that wraps the three calls we actually need:
//! construct, load-from-resource, and install-onto-the-app. The translator
//! is a global `static mut` because it has to outlive QML evaluation —
//! freeing it would invalidate Qt's bound translation pointers.
//!
//! Strings are loaded from `qrc:/i18n/usbooty_<locale>.qm`, embedded into
//! the binary by `crates/gui/qrc/translations.qrc` at build time.

use cxx::let_cxx_string;
use std::sync::Mutex;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("translator_bridge.h");

        type QTranslator;
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        /// Heap-allocate a translator, returns the raw pointer. The caller
        /// owns the allocation and is responsible for keeping it alive for
        /// the lifetime of the Qt application.
        fn translator_new() -> *mut QTranslator;

        /// Load a `.qm` file from the given path (a `qrc:/…` URL is OK).
        /// Returns whether the load succeeded.
        unsafe fn translator_load(tr: *mut QTranslator, path: &CxxString) -> bool;

        /// Install the translator onto the global QCoreApplication so
        /// `qsTr` / `QObject::tr` start using it. Must run after a
        /// `QGuiApplication` exists.
        unsafe fn translator_install(tr: *mut QTranslator);

        /// Remove a previously-installed translator. Qt emits a
        /// LanguageChange event after this, which QML's `qsTr` bindings
        /// pick up and re-evaluate, so the UI live-switches without a
        /// restart.
        unsafe fn translator_remove(tr: *mut QTranslator);

        /// Free the translator allocation.
        unsafe fn translator_delete(tr: *mut QTranslator);

        /// Force every QML `qsTr()` binding on `engine` to re-evaluate.
        /// QCoreApplication does fire LanguageChange after install /
        /// removeTranslator, but in practice QML's TranslationBindings
        /// only refresh reliably when QQmlEngine::retranslate() is invoked
        /// explicitly.
        unsafe fn engine_retranslate(engine: *mut QQmlApplicationEngine);
    }
}

/// Raw pointer to the currently-installed translator, or null when the UI is
/// in source-language (English) mode. Stored as a usize because raw
/// pointers aren't `Send` — wrapped in a Mutex so the GUI thread can swap
/// it from menu callbacks.
static CURRENT: Mutex<usize> = Mutex::new(0);

/// Raw pointer to the QQmlApplicationEngine `main.rs` constructed, kept so
/// the language toggle can call `engine.retranslate()` after the
/// install/removeTranslator swap. Zero until `register_engine` is called.
static ENGINE: Mutex<usize> = Mutex::new(0);

/// Hand the engine pointer to the translation module so toggling the
/// language at runtime can ping it. Call once, right after creating the
/// QQmlApplicationEngine in `main.rs`.
pub fn register_engine(engine: *mut ffi::QQmlApplicationEngine) {
    *ENGINE.lock().expect("engine lock poisoned") = engine as usize;
}

/// Pick the best `.qm` file for the current system locale and install it
/// onto the global QCoreApplication. Best-effort — a missing translation
/// (no matching `.qm`, the user runs a locale we don't ship) silently
/// falls back to the English source strings baked into `qsTr` calls.
///
/// Must be called *after* the QGuiApplication is constructed.
pub fn install_for_system_locale() {
    let locale_full = system_locale(); // e.g. "fr_FR"
    let short: String = locale_full.split('_').next().unwrap_or("en").to_string();
    for candidate in [locale_full.as_str(), short.as_str()] {
        if candidate == "en" {
            return;
        }
        if load_and_install(candidate) {
            return;
        }
    }
}

/// Swap to English (no translator) or back to the system locale at runtime.
/// QCoreApplication::removeTranslator / installTranslator both emit a
/// QEvent::LanguageChange, which the QML engine forwards to every `qsTr`
/// binding — so the UI re-renders in the new language without restart.
///
/// `force_english=true` strips the current translator so qsTr returns its
/// English source strings. `false` re-installs the system-locale .qm.
pub fn set_force_english(force_english: bool) {
    unsafe {
        let mut slot = CURRENT.lock().expect("translator lock poisoned");
        // Always remove the previous translator first; that triggers the
        // LanguageChange event regardless of which way we're switching.
        if *slot != 0 {
            let old = *slot as *mut ffi::QTranslator;
            ffi::translator_remove(old);
            ffi::translator_delete(old);
            *slot = 0;
        }
        if !force_english {
            // Re-derive the locale every time — if the user changed it in
            // their session env, we'll pick it up on toggle-off.
            drop(slot);
            install_for_system_locale();
        }
        // Force every qsTr binding to re-evaluate; QML's TranslationBinding
        // doesn't always pick up the bare LanguageChange event.
        let engine = *ENGINE.lock().expect("engine lock poisoned");
        if engine != 0 {
            ffi::engine_retranslate(engine as *mut ffi::QQmlApplicationEngine);
        }
    }
}

/// Load `usbooty_<locale>.qm` from the baked-in resource bundle and
/// install it. Records the translator pointer in `CURRENT` so it can be
/// removed by [`set_force_english`] later. Returns whether the load
/// succeeded.
fn load_and_install(locale: &str) -> bool {
    let path = format!(":/i18n/usbooty_{locale}.qm");
    unsafe {
        let tr = ffi::translator_new();
        let_cxx_string!(c_path = path.clone());
        if ffi::translator_load(tr, &c_path) {
            ffi::translator_install(tr);
            *CURRENT.lock().expect("translator lock poisoned") = tr as usize;
            eprintln!("usbooty: loaded translation {path}");
            true
        } else {
            ffi::translator_delete(tr);
            false
        }
    }
}

/// `LC_ALL` → `LC_MESSAGES` → `LANG` → "en_US". Mirrors what `QLocale`
/// does internally, but without needing a QLocale binding.
fn system_locale() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            if !value.is_empty() && value != "C" && value != "POSIX" {
                // "fr_FR.UTF-8" → "fr_FR"
                return value.split(['.', '@']).next().unwrap_or(&value).to_string();
            }
        }
    }
    "en_US".to_string()
}

