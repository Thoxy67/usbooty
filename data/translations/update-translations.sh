#!/usr/bin/env bash
# Regenerate .ts files from QML sources, then compile each to .qm.
# Run from anywhere in the repo:
#   ./data/translations/update-translations.sh
#
# Translators: open the .ts file in `linguist6` (`pacman -S qt6-tools` or
# the equivalent on your distro), fill in <translation> elements, save,
# re-run this script to compile to .qm and reload the GUI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Resolve a Qt6 linguist tool across distro layouts: Arch ships `lupdate6`
# on PATH, openSUSE uses `lupdate-qt6`, Debian/Ubuntu hide the unsuffixed
# binaries in /usr/lib/qt6/bin (qt6-l10n-tools).
find_qt6_tool() {
    local tool="$1" c
    for c in "${tool}6" "${tool}-qt6"; do
        if command -v "$c" >/dev/null 2>&1; then echo "$c"; return 0; fi
    done
    for c in "/usr/lib/qt6/bin/$tool" "/usr/lib64/qt6/bin/$tool"; do
        if [ -x "$c" ]; then echo "$c"; return 0; fi
    done
    if command -v "$tool" >/dev/null 2>&1 \
        && "$tool" -version 2>/dev/null | grep -q 'version 6\.'; then
        echo "$tool"; return 0
    fi
    echo "ERROR: Qt6 $tool not found; install qt6-tools (Arch) / qt6-l10n-tools (Debian)." >&2
    return 1
}
LUPDATE="$(find_qt6_tool lupdate)"
LRELEASE="$(find_qt6_tool lrelease)"

# Locales we ship; add new ones here. The qrc/translations.qrc must also
# gain a matching <file alias="usbooty_<loc>.qm"> entry.
LOCALES=(fr)

# 1. Update .ts files: lupdate scans the QML tree and merges new strings
#    into existing .ts entries while preserving translator-completed ones.
for loc in "${LOCALES[@]}"; do
    echo "==> lupdate $loc"
    "$LUPDATE" -recursive "$REPO_ROOT/crates/gui/qml" \
               -ts "$SCRIPT_DIR/usbooty_${loc}.ts"
done

# 2. Compile each .ts → .qm. lrelease writes the compiled binary next to
#    the source file; cxx-qt-build embeds it via qrc/translations.qrc.
echo "==> lrelease"
"$LRELEASE" "$SCRIPT_DIR"/usbooty_*.ts

echo
echo "Done. Rebuild the GUI to pick up the new translations."
