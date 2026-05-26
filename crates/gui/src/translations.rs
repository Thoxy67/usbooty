//! Locale-aware translation loading via `QTranslator`.
//!
//! Wraps six functions from `include/translator_bridge.cpp` through a
//! plain `extern "C"` ABI. Going through cxx / cxx-qt was the previous
//! design, but cxx-qt-build's `.file()` doesn't pick up plain
//! `#[cxx::bridge]` modules (it only processes cxx-qt bridges), so a
//! clean build environment (like makepkg's chroot) ends up with the
//! bridge stubs unresolved. The raw `extern "C"` approach + a small
//! C++ shim compiled by the `cc` crate works everywhere.
//!
//! `.qm` files are loaded from `qrc:/i18n/usbooty_<locale>.qm`, embedded
//! into the binary by `crates/gui/qrc/translations.qrc` at build time.

use std::ffi::{CString, c_char, c_void};
use std::sync::Mutex;

// The C++ side is opaque to Rust; we juggle raw pointers and let the C++
// code own the QTranslator / QQmlApplicationEngine instances.
#[repr(C)]
struct QTranslator {
    _opaque: [u8; 0],
}
#[repr(C)]
struct QQmlApplicationEngine {
    _opaque: [u8; 0],
}

// Edition 2024 requires `extern` blocks to be marked `unsafe`: the compiler
// can't verify the signatures match the C side, so importing them is the
// unsafe act, while each individual call is still `unsafe { ... }` at use.
unsafe extern "C" {
    fn usbooty_translator_new() -> *mut QTranslator;
    fn usbooty_translator_load(tr: *mut QTranslator, path: *const c_char) -> bool;
    fn usbooty_translator_install(tr: *mut QTranslator);
    fn usbooty_translator_remove(tr: *mut QTranslator);
    fn usbooty_translator_delete(tr: *mut QTranslator);
    fn usbooty_engine_retranslate(engine: *mut QQmlApplicationEngine);
}

/// Raw pointer to the currently-installed translator, or null when the UI
/// is in source-language (English) mode. Stored as a usize because raw
/// pointers aren't `Send`; a Mutex serialises menu-driven swaps.
static CURRENT: Mutex<usize> = Mutex::new(0);

/// Raw pointer to the QQmlApplicationEngine `main.rs` constructed, kept so
/// the language toggle can call `engine.retranslate()` after the
/// install/removeTranslator swap. Zero until `register_engine` is called.
static ENGINE: Mutex<usize> = Mutex::new(0);

/// Hand the engine pointer to the translation module so toggling the
/// language at runtime can ping it. Call once, right after creating the
/// QQmlApplicationEngine in `main.rs`. The pointer must remain valid for
/// the lifetime of the application.
pub fn register_engine(engine: *mut c_void) {
    *ENGINE.lock().expect("engine lock poisoned") = engine as usize;
}

/// Pick the best `.qm` file for the current system locale and install it
/// onto the global QCoreApplication. Best-effort — a missing translation
/// (no matching `.qm`, the user runs a locale we don't ship) silently
/// falls back to the English source strings baked into `qsTr` calls.
///
/// Must be called *after* the QGuiApplication is constructed.
pub fn install_for_system_locale() {
    let locale_full = system_locale();
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
/// QEvent::LanguageChange; we also explicitly retranslate the QML engine
/// so every `qsTr` binding re-evaluates.
pub fn set_force_english(force_english: bool) {
    {
        let mut slot = CURRENT.lock().expect("translator lock poisoned");
        if *slot != 0 {
            unsafe {
                let old = *slot as *mut QTranslator;
                usbooty_translator_remove(old);
                usbooty_translator_delete(old);
            }
            *slot = 0;
        }
    }
    if !force_english {
        install_for_system_locale();
    }
    let engine = *ENGINE.lock().expect("engine lock poisoned");
    if engine != 0 {
        unsafe { usbooty_engine_retranslate(engine as *mut QQmlApplicationEngine) };
    }
}

/// Load `usbooty_<locale>.qm` from the baked-in resource bundle and
/// install it. Records the translator pointer so [`set_force_english`]
/// can remove it later. Returns whether the load succeeded.
fn load_and_install(locale: &str) -> bool {
    let path = format!(":/i18n/usbooty_{locale}.qm");
    let c_path = CString::new(path.clone()).expect("path has no interior NUL");
    unsafe {
        let tr = usbooty_translator_new();
        if usbooty_translator_load(tr, c_path.as_ptr()) {
            usbooty_translator_install(tr);
            *CURRENT.lock().expect("translator lock poisoned") = tr as usize;
            eprintln!("usbooty: loaded translation {path}");
            true
        } else {
            usbooty_translator_delete(tr);
            false
        }
    }
}

/// `LC_ALL` → `LC_MESSAGES` → `LANG` → "en_US". Mirrors what `QLocale`
/// does internally, but without needing a QLocale binding.
fn system_locale() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
            && value != "C"
            && value != "POSIX"
        {
            return value.split(['.', '@']).next().unwrap_or(&value).to_string();
        }
    }
    "en_US".to_string()
}
