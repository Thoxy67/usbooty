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

# Locales we ship — add new ones here; the qrc/translations.qrc must also
# gain a matching <file alias="usbooty_<loc>.qm"> entry.
LOCALES=(fr)

# 1. Update .ts files: lupdate scans the QML tree and merges new strings
#    into existing .ts entries while preserving translator-completed ones.
for loc in "${LOCALES[@]}"; do
    echo "==> lupdate $loc"
    lupdate6 -recursive "$REPO_ROOT/crates/gui/qml" \
             -ts "$SCRIPT_DIR/usbooty_${loc}.ts"
done

# 2. Compile each .ts → .qm. lrelease writes the compiled binary next to
#    the source file; cxx-qt-build embeds it via qrc/translations.qrc.
echo "==> lrelease"
lrelease6 "$SCRIPT_DIR"/usbooty_*.ts

echo
echo "Done. Rebuild the GUI to pick up the new translations."
