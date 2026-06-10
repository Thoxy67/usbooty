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
ts="$REPO_ROOT/data/translations/usbooty_fr.ts"
qm="$REPO_ROOT/data/translations/usbooty_fr.qm"
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
trap 'rm -f "$tmp_ts"' EXIT
cp "$ts" "$tmp_ts"
if ! lupdate6 -recursive "$REPO_ROOT/crates/gui/qml" -ts "$tmp_ts" >/dev/null 2>&1; then
    echo "ERROR: lupdate6 failed to scan the QML tree." >&2
    exit 1
fi
if grep -q 'type="unfinished"' "$tmp_ts"; then
    echo "ERROR: $ts is stale: QML has new/changed qsTr strings." >&2
    echo "Run data/translations/update-translations.sh and translate them:" >&2
    grep -n 'type="unfinished"' "$tmp_ts" | head -5 >&2
    exit 1
fi
if ! lrelease6 "$ts" >/dev/null 2>&1; then
    echo "ERROR: lrelease6 failed to compile $ts." >&2
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
