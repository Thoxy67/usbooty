#!/bin/sh
# Install usbooty: builds the release binaries and places them, the polkit
# policy, and the desktop integration files into the system directories.
#
# Usage: sudo ./install.sh
set -eu

PREFIX="${PREFIX:-/usr}"
LIBEXEC="$PREFIX/libexec/usbooty"
SRC_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ "$(id -u)" -ne 0 ]; then
    echo "This script must be run as root (e.g. sudo ./install.sh)." >&2
    exit 1
fi

echo "Building release binaries..."
( cd "$SRC_DIR" && cargo build --release )

TARGET="$SRC_DIR/target/release"

echo "Installing binaries..."
install -Dm755 "$TARGET/usbooty"        "$PREFIX/bin/usbooty"
install -Dm755 "$TARGET/usbooty-helper" "$LIBEXEC/usbooty-helper"

echo "Installing polkit policy..."
install -Dm644 "$SRC_DIR/data/org.usbooty.helper.policy" \
    "$PREFIX/share/polkit-1/actions/org.usbooty.helper.policy"

echo "Installing desktop integration..."
install -Dm644 "$SRC_DIR/data/org.usbooty.Usbooty.desktop" \
    "$PREFIX/share/applications/org.usbooty.Usbooty.desktop"
install -Dm644 "$SRC_DIR/data/org.usbooty.Usbooty.metainfo.xml" \
    "$PREFIX/share/metainfo/org.usbooty.Usbooty.metainfo.xml"
install -Dm644 "$SRC_DIR/data/icons/org.usbooty.Usbooty.svg" \
    "$PREFIX/share/icons/hicolor/scalable/apps/org.usbooty.Usbooty.svg"

echo "usbooty installed. Launch it from your application menu or run 'usbooty'."
