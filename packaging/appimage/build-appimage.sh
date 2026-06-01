#!/usr/bin/env bash
# Build an AppImage of usbooty using linuxdeploy + the Qt plugin.
#
# Run from the repo root (the script auto-locates itself otherwise):
#   ./packaging/appimage/build-appimage.sh
#
# Output: ./usbooty-<arch>.AppImage
#
# Prereqs on the build host:
#   * Rust stable
#   * Qt 6 development headers (qt6-base, qt6-declarative)
#   * pkgconf, fuse2 (linuxdeploy needs FUSE to test-run the AppImage)
#   * curl, wget
#
# The AppImage carries the Qt 6 runtime + the QML modules usbooty uses,
# so it Just Works on any glibc-2.31+ host (Ubuntu 20.04 LTS or newer).
# Host-side optional dependencies (mkfs.*, ventoy, smartctl, …) still need
# to be installed on the target machine; see packaging/appimage/README.md.

set -euo pipefail

# --- Locate the repo root ---------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

ARCH="$(uname -m)"
BUILD_DIR="$REPO_ROOT/target/appimage"
APPDIR="$BUILD_DIR/AppDir"
TOOLS_DIR="$BUILD_DIR/tools"

mkdir -p "$BUILD_DIR" "$TOOLS_DIR"

# --- Fetch linuxdeploy + the Qt plugin --------------------------------------
LD_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${ARCH}.AppImage"
LDQT_URL="https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-${ARCH}.AppImage"

fetch() {
    local url="$1" out="$2"
    if [[ ! -x "$out" ]]; then
        echo "==> Fetching $(basename "$out") …"
        curl -L --fail -o "$out" "$url"
        chmod +x "$out"
    fi
}
fetch "$LD_URL"   "$TOOLS_DIR/linuxdeploy"
fetch "$LDQT_URL" "$TOOLS_DIR/linuxdeploy-plugin-qt"

# --- Build usbooty ----------------------------------------------------------
echo "==> Building release binaries"
cargo build --release --locked

# --- Stage the AppDir -------------------------------------------------------
echo "==> Staging AppDir at $APPDIR"
rm -rf "$APPDIR"
install -Dm755 target/release/usbooty          "$APPDIR/usr/bin/usbooty"
install -Dm755 target/release/usbooty-helper   "$APPDIR/usr/libexec/usbooty/usbooty-helper"
install -Dm644 data/org.usbooty.Usbooty.desktop \
    "$APPDIR/usr/share/applications/org.usbooty.Usbooty.desktop"
install -Dm644 data/org.usbooty.Usbooty.metainfo.xml \
    "$APPDIR/usr/share/metainfo/org.usbooty.Usbooty.metainfo.xml"
install -Dm644 data/icons/org.usbooty.Usbooty.svg \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps/org.usbooty.Usbooty.svg"
install -Dm644 data/org.usbooty.helper.policy \
    "$APPDIR/usr/share/polkit-1/actions/org.usbooty.helper.policy"

# linuxdeploy reads these from the AppDir root as fallback.
cp data/icons/org.usbooty.Usbooty.svg "$APPDIR/org.usbooty.Usbooty.svg"
cp data/org.usbooty.Usbooty.desktop   "$APPDIR/org.usbooty.Usbooty.desktop"

# --- AppRun shim ------------------------------------------------------------
# The helper inside the AppImage is found via $APPDIR; the runner already
# looks next to the GUI binary first, so a small wrapper isn't strictly
# required. We do export QT_QPA_PLATFORM_PLUGIN_PATH so xcb / wayland
# integration works on hosts whose Qt env differs from ours.
cat >"$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HERE/usr/lib:${LD_LIBRARY_PATH:-}"
export QT_PLUGIN_PATH="$HERE/usr/plugins:${QT_PLUGIN_PATH:-}"
export QML2_IMPORT_PATH="$HERE/usr/qml:${QML2_IMPORT_PATH:-}"
exec "$HERE/usr/bin/usbooty" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# --- Bundle Qt + QML, then package an AppImage ------------------------------
echo "==> Bundling Qt and packaging AppImage"
export NO_STRIP=1   # AppImage's strip can corrupt PyInstaller-style ELFs;
                    # safer to keep symbols.
export LD_LIBRARY_PATH=""
export QML_SOURCES_PATHS="$REPO_ROOT/crates/gui/qml"

# linuxdeploy-plugin-qt picks qmake from $QMAKE first, falling back to
# whatever `qmake` is on $PATH, which on Arch is `qmake-qt5`, so without
# this override the plugin tries to deploy Qt-5 modules against our Qt-6
# binary and bails with "Could not find Qt modules to deploy".
if [[ -z "${QMAKE:-}" ]]; then
    for cand in /usr/lib/qt6/bin/qmake /usr/bin/qmake6 /usr/sbin/qmake6 /usr/lib64/qt6/bin/qmake; do
        if [[ -x "$cand" ]]; then
            export QMAKE="$cand"
            break
        fi
    done
fi
if [[ -z "${QMAKE:-}" ]]; then
    echo "!! Could not find a Qt 6 qmake on this system."
    echo "   Install qt6-base (Arch / Fedora) or qt6-base-dev (Debian)," \
         "or set QMAKE=/path/to/qmake6 and re-run." >&2
    exit 1
fi
echo "==> Using qmake at $QMAKE ($($QMAKE -query QT_VERSION))"

# Mirror Qt's plugin tree so we can drop optional add-ons with missing
# backing libraries (e.g. `kimg_jxr.so` from kimageformats needs libjxr,
# which is an optdep most hosts don't have installed). The mirror uses
# symlinks so it's near-free; we then hide the broken plugins by deleting
# the *links* in the mirror, never touching the real Qt install.
REAL_QMAKE="$QMAKE"
REAL_PLUGINS_DIR="$("$REAL_QMAKE" -query QT_INSTALL_PLUGINS)"
PLUGINS_MIRROR="$BUILD_DIR/qt-plugins"
rm -rf "$PLUGINS_MIRROR"
mkdir -p "$PLUGINS_MIRROR"
cp -rs "$REAL_PLUGINS_DIR/." "$PLUGINS_MIRROR/"

# The k-image-formats Qt 6 add-on contributes 9-ish plugins for niche
# image formats (jxr, heic, ora, …). Qt's built-in handlers cover PNG /
# JPEG / SVG / etc., which is everything usbooty actually renders.
# Dropping the add-ons sidesteps every "Could not find dependency: libX"
# from linuxdeploy when those add-ons' backing libs are missing.
find "$PLUGINS_MIRROR/imageformats" -maxdepth 1 -name 'kimg_*.so' \
    -delete 2>/dev/null || true

# Wrapper qmake that returns the mirror path for QT_INSTALL_PLUGINS and
# delegates everything else to the real qmake. linuxdeploy-plugin-qt does
# `qmake -query QT_INSTALL_PLUGINS` to find what to scan, so this is the
# one knob that matters.
QMAKE_WRAPPER="$BUILD_DIR/qmake-wrapper"
cat >"$QMAKE_WRAPPER" <<EOF
#!/bin/sh
# Two forms of qmake invocation matter to linuxdeploy-plugin-qt:
#   * \`qmake -query QT_INSTALL_PLUGINS\`: specific lookup
#   * \`qmake -query\`                   : bulk dump that callers grep
# Both have to surface the mirror so the scanner walks our filtered tree.
case "\$*" in
    "-query QT_INSTALL_PLUGINS")
        printf '%s\n' "$PLUGINS_MIRROR"
        exit 0
        ;;
    "-query")
        "$REAL_QMAKE" -query \
            | sed "s|^QT_INSTALL_PLUGINS:.*|QT_INSTALL_PLUGINS:$PLUGINS_MIRROR|"
        exit 0
        ;;
esac
exec "$REAL_QMAKE" "\$@"
EOF
chmod +x "$QMAKE_WRAPPER"
export QMAKE="$QMAKE_WRAPPER"

"$TOOLS_DIR/linuxdeploy" \
    --appdir "$APPDIR" \
    --plugin qt \
    --output appimage

mv -f "usbooty-${ARCH}.AppImage" "$REPO_ROOT/" 2>/dev/null || true

echo
echo "==> Done. AppImage at: $REPO_ROOT/usbooty-${ARCH}.AppImage"
