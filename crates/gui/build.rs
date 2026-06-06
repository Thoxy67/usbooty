use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
use std::path::Path;
use std::process::Command;

fn main() {
    // Strip LTO from the C++ flags before anything else runs. cc::Build
    // (used internally by cxx-qt-build) honours CXXFLAGS / CFLAGS from
    // the environment; on Arch + CachyOS-style makepkg.conf those carry
    // `-flto=auto`, which makes every .cpp compile to LLVM bitcode. Rust
    // is built without LTO (see Cargo.toml's profile.release), so the
    // final rust-lld pass can't read the bitcode and reports every
    // cxx-qt / cxx-bridge symbol as undefined. makepkg's `!lto` option
    // does not remove `-flto=auto` on every distro config, so do it
    // ourselves to make the build robust everywhere.
    strip_lto_from_env("CXXFLAGS");
    strip_lto_from_env("CFLAGS");

    // Compile any `.ts` translation source whose `.qm` is missing or
    // stale before `qrc/translations.qrc` is processed by rcc. `.qm`
    // files are gitignored, so a fresh clone never has them; doing the
    // lrelease step here means `cargo build` works on its own, with no
    // separate setup step in the AUR PKGBUILD, AppImage script, or for
    // contributors. Missing `lrelease6` is fatal; we *need* the .qm
    // for the qrc embed to succeed.
    compile_translations(Path::new("../../data/translations"));

    println!("cargo:rerun-if-changed=include/translator_bridge.h");
    println!("cargo:rerun-if-changed=include/translator_bridge.cpp");

    CxxQtBuilder::new_qml_module(
        QmlModule::new("com.usbooty")
            // main.qml is the window shell; the rest of the UI lives in
            // sibling files of the same module, reachable by type name with
            // no extra import. Ui is a singleton holding shared helpers.
            .qml_file(QmlFile::from("qml/Ui.qml").singleton(true))
            .qml_files([
                "qml/main.qml",
                // Reusable leaf components.
                "qml/components/StepCard.qml",
                "qml/components/WindowsLogo.qml",
                "qml/components/LinuxLogo.qml",
                "qml/components/Pill.qml",
                "qml/components/DialogHeader.qml",
                "qml/components/FormCombo.qml",
                "qml/components/WrapCheckBox.qml",
                "qml/components/Banner.qml",
                "qml/components/SplitButton.qml",
                // Top-level dialogs.
                "qml/dialogs/CheckConfirmDialog.qml",
                "qml/dialogs/WindowsSetupDialog.qml",
                "qml/dialogs/InspectDialog.qml",
                "qml/dialogs/DepsDialog.qml",
                "qml/dialogs/BootTestDialog.qml",
                "qml/dialogs/ResultDialog.qml",
                "qml/dialogs/AboutDialog.qml",
                "qml/dialogs/EraseConfirmDialog.qml",
                "qml/dialogs/WindowsDownloadDialog.qml",
            ]),
    )
        .file("src/bridge/mod.rs")
        // Compile the QTranslator C++ shim *through cxx-qt-build*, using
        // a separate cc::Build invocation works locally with stale
        // incremental artefacts but fails in a clean build because the
        // two builders overwrite each other's link-arg state in OUT_DIR.
        // cpp_file() lets cxx-qt-build add our .cpp to the same compile
        // batch as its own glue, so Qt include flags, ABI flags and link
        // ordering all match.
        .cpp_file("include/translator_bridge.cpp")
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

/// Remove every `-flto*` token from the named env var, in place.
/// Anchored to `-flto` (matches `-flto`, `-flto=auto`, `-flto=thin`, …)
/// so we don't accidentally strip something else.
fn strip_lto_from_env(var: &str) {
    let Ok(value) = std::env::var(var) else {
        return;
    };
    let cleaned: String = value
        .split_whitespace()
        .filter(|tok| !tok.starts_with("-flto"))
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned != value {
        // SAFETY: build scripts run single-threaded before any user code, so
        // the data-race concern that made `set_var` unsafe in edition 2024
        // does not apply here.
        unsafe { std::env::set_var(var, &cleaned) };
        println!("cargo:warning=stripped -flto from {var} ({value:?} -> {cleaned:?})");
    }
}

/// Run `lrelease6` over every `.ts` in `dir` whose matching `.qm` is
/// missing or older than the `.ts`. Best-effort: succeeds when every
/// expected `.qm` ends up on disk; panics with a clear message otherwise
/// so the failure happens here instead of buried in rcc's output.
fn compile_translations(dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("could not list {}: {e}", dir.display()),
    };
    let mut ts_files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "ts") {
            println!("cargo:rerun-if-changed={}", path.display());
            ts_files.push(path);
        }
    }
    if ts_files.is_empty() {
        return;
    }

    // Locate lrelease6. Arch ships it in /usr/bin (qt6-tools); some
    // distros use /usr/lib/qt6/bin/lrelease. The plain `lrelease` is
    // sometimes Qt 5; prefer the explicit Qt 6 name.
    let lrelease = ["lrelease6", "/usr/lib/qt6/bin/lrelease"]
        .into_iter()
        .find(|bin| {
            Command::new(bin)
                .arg("-version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!(
                "lrelease6 not found on PATH or /usr/lib/qt6/bin; install \
                 qt6-tools (Arch) / qttools-dev-tools (Debian/Ubuntu) to \
                 compile the translation .ts files into .qm before the qrc \
                 step embeds them."
            )
        });

    // Each `.ts` compiles into its own `.qm` independently, so spawn all the
    // stale ones at once and collect their handles, then wait. On a fresh
    // clone (every `.qm` missing) this turns N sequential lrelease runs into
    // one concurrent batch.
    let mut jobs = Vec::new();
    for ts in &ts_files {
        let qm = ts.with_extension("qm");
        let needs_rebuild = match (std::fs::metadata(&qm), std::fs::metadata(ts)) {
            (Ok(qm_meta), Ok(ts_meta)) => match (qm_meta.modified(), ts_meta.modified()) {
                (Ok(qm_t), Ok(ts_t)) => qm_t < ts_t,
                _ => true,
            },
            _ => true,
        };
        if !needs_rebuild {
            continue;
        }
        let child = Command::new(lrelease)
            .arg(ts)
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {lrelease}: {e}"));
        jobs.push((ts, child));
    }
    for (ts, mut child) in jobs {
        let status = child
            .wait()
            .unwrap_or_else(|e| panic!("failed to wait for {lrelease}: {e}"));
        if !status.success() {
            panic!("{lrelease} {} failed (exit {status})", ts.display());
        }
    }
}
