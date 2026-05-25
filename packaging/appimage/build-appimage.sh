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
# to be installed on the target machine — see packaging/appimage/README.md.

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
"$TOOLS_DIR/linuxdeploy" \
    --appdir "$APPDIR" \
    --plugin qt \
    --output appimage

mv -f "usbooty-${ARCH}.AppImage" "$REPO_ROOT/" 2>/dev/null || true

echo
echo "==> Done. AppImage at: $REPO_ROOT/usbooty-${ARCH}.AppImage"
