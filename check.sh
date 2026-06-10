#!/usr/bin/env bash
# Pre-PR tripwire: every check that must pass before pushing a branch.
#
# Run from the repo root:
#
#   ./check.sh
#
# Exits non-zero on the first failure so CI / pre-push hooks can short-circuit.
# This is the single source of truth for "is the tree healthy?" and is invoked
# verbatim by .forgejo/workflows/check.yml on push + PR to main.

set -euo pipefail

# Resolve to the repo root regardless of where the user invoked us from.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_ROOT"

step() { printf '\n\033[1;36m==>\033[0m %s\n' "$1"; }

# Note: cargo fmt is intentionally NOT enforced here. The tree predates a
# global rustfmt pass; introducing one as a tripwire would turn every PR
# into a drive-by reformat. Run `cargo fmt --all` manually if you want
# a clean diff, but the gate stays on clippy + tests + translations.

step "cargo clippy (denying every warning)"
cargo clippy --workspace --all-targets --locked -- -D warnings

# Real build in normal (non-test) mode. `cargo test` would catch most errors
# but compiles every crate with cfg(test) set, so any code gated on
# #[cfg(not(test))] is only exercised here.
step "cargo build --workspace"
cargo build --workspace --locked

step "cargo test --workspace"
cargo test --workspace --locked

step "translation catalog is finished, current, and compiles"
# Resolve a Qt6 linguist tool across distro layouts: Arch ships `lupdate6`
# on PATH, openSUSE uses `lupdate-qt6`, Debian/Ubuntu hide the unsuffixed
# binaries in /usr/lib/qt6/bin (qt6-l10n-tools). A bare name on PATH is
# accepted last, and only if it reports Qt 6 (a Qt 5 lupdate would silently
# mis-scan the catalog).
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
    return 1
}
LUPDATE="$(find_qt6_tool lupdate)" || {
    echo "ERROR: Qt6 lupdate not found; install qt6-tools (Arch) / qt6-l10n-tools (Debian)." >&2
    exit 1
}
LRELEASE="$(find_qt6_tool lrelease)" || {
    echo "ERROR: Qt6 lrelease not found; install qt6-tools (Arch) / qt6-l10n-tools (Debian)." >&2
    exit 1
}
ts="$REPO_ROOT/data/translations/usbooty_fr.ts"
if grep -q 'type="unfinished"' "$ts"; then
    echo "ERROR: $ts has unfinished translations." >&2
    grep -n 'type="unfinished"' "$ts" | head -5 >&2
    exit 1
fi
# Catch a *stale* catalog too: a qsTr string added or changed in QML but
# never run through update-translations.sh is simply absent from the
# committed .ts, so the grep above cannot see it. Re-running lupdate into a
# scratch copy makes any such string show up as a fresh unfinished entry.
tmp_ts="$(mktemp --suffix=.ts)"
tmp_qm="$(mktemp --suffix=.qm)"
trap 'rm -f "$tmp_ts" "$tmp_qm"' EXIT
cp "$ts" "$tmp_ts"
if ! out="$("$LUPDATE" -recursive "$REPO_ROOT/crates/gui/qml" -ts "$tmp_ts" 2>&1)"; then
    echo "ERROR: $LUPDATE failed to scan the QML tree:" >&2
    echo "$out" >&2
    exit 1
fi
if grep -q 'type="unfinished"' "$tmp_ts"; then
    echo "ERROR: $ts is stale: QML has new/changed qsTr strings." >&2
    echo "Run data/translations/update-translations.sh and translate them:" >&2
    grep -n 'type="unfinished"' "$tmp_ts" | head -5 >&2
    exit 1
fi
# Compile to a scratch .qm: this is a syntax gate, not a build step, so it
# must not mutate the working tree (the real .qm is built by build.rs).
if ! out="$("$LRELEASE" "$ts" -qm "$tmp_qm" 2>&1)"; then
    echo "ERROR: $LRELEASE failed to compile $ts:" >&2
    echo "$out" >&2
    exit 1
fi

step "docs contain no em-dash characters"
# -I skips binary files; without it, the U+2014 UTF-8 byte sequence (e2 80
# 94) appears by chance inside screenshots / PDFs and trips the rule for
# bogus reasons. Binary assets cannot meaningfully "contain" prose, so
# excluding them keeps the check on text-only docs where it belongs.
if grep -rnI $'\xe2\x80\x94' docs/ >/dev/null 2>&1; then
    echo "ERROR: em-dash character found in docs/ (rule: no em-dashes)." >&2
    grep -rnI $'\xe2\x80\x94' docs/ >&2
    exit 1
fi

printf '\n\033[1;32mAll checks passed.\033[0m\n'
