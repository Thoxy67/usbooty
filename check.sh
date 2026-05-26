#!/usr/bin/env bash
# Pre-PR tripwire: every check that must pass before pushing a branch.
#
# Run from the repo root:
#
#   ./check.sh
#
# Exits non-zero on the first failure so CI / pre-push hooks can short-circuit.
# This is the single source of truth for "is the tree healthy?"; there is no
# GitHub Actions or Forgejo Workflow file at the moment.

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

step "cargo test --workspace"
cargo test --workspace --locked

step "translation catalog is finished and compiles"
ts="$REPO_ROOT/data/translations/usbooty_fr.ts"
qm="$REPO_ROOT/data/translations/usbooty_fr.qm"
if grep -q 'type="unfinished"' "$ts"; then
    echo "ERROR: $ts has unfinished translations." >&2
    grep -n 'type="unfinished"' "$ts" | head -5 >&2
    exit 1
fi
if ! lrelease6 "$ts" >/dev/null 2>&1; then
    echo "ERROR: lrelease6 failed to compile $ts." >&2
    exit 1
fi

step "docs contain no em-dash characters"
if grep -rn $'\xe2\x80\x94' docs/ >/dev/null 2>&1; then
    echo "ERROR: em-dash character found in docs/ (rule: no em-dashes)." >&2
    grep -rn $'\xe2\x80\x94' docs/ >&2
    exit 1
fi

printf '\n\033[1;32mAll checks passed.\033[0m\n'
