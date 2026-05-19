#!/bin/sh
# publish-aur.sh — copy PKGBUILD + .install + README into a checkout of the
# separate AUR repo for usbooty-git, regenerate .SRCINFO, show a diff, and
# optionally commit + push.
#
# The PKGBUILD in this repo is the *canonical source*; the AUR repo is a
# downstream that only contains PKGBUILD, .install and .SRCINFO. This script
# keeps them in sync.
#
# Usage:
#   ./publish-aur.sh              # update working copy, show diff, don't push
#   ./publish-aur.sh --push       # also commit + push to origin/master
#   ./publish-aur.sh --dir /tmp/x # use a custom working copy location
#
# Run from anywhere; the script resolves paths from its own location.

set -eu

AUR_REMOTE_DEFAULT="ssh://git@git.thoxy.xyz:222/AUR/usbooty-git.git"
AUR_REMOTE="${AUR_REMOTE:-$AUR_REMOTE_DEFAULT}"
AUR_DIR_DEFAULT="$HOME/dev/aur/usbooty-git"

DO_PUSH=0
AUR_DIR="$AUR_DIR_DEFAULT"

while [ $# -gt 0 ]; do
    case "$1" in
        --push)   DO_PUSH=1 ;;
        --dir)    shift; AUR_DIR="$1" ;;
        --remote) shift; AUR_REMOTE="$1" ;;
        -h|--help)
            sed -n '2,/^$/p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            printf 'unknown flag: %s\n' "$1" >&2
            exit 2
            ;;
    esac
    shift
done

PACKAGING_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_PKGBUILD="$PACKAGING_DIR/PKGBUILD"
SRC_INSTALL="$PACKAGING_DIR/usbooty-git.install"
SRC_README="$PACKAGING_DIR/aur-README.md"

[ -f "$SRC_PKGBUILD" ] || { echo "missing $SRC_PKGBUILD" >&2; exit 1; }
[ -f "$SRC_INSTALL"  ] || { echo "missing $SRC_INSTALL"  >&2; exit 1; }
[ -f "$SRC_README"   ] || { echo "missing $SRC_README"   >&2; exit 1; }

# 1. Make sure we have a working copy of the AUR repo.
if [ ! -d "$AUR_DIR/.git" ]; then
    echo "==> Cloning $AUR_REMOTE -> $AUR_DIR"
    mkdir -p "$(dirname "$AUR_DIR")"
    git clone "$AUR_REMOTE" "$AUR_DIR"
else
    echo "==> Updating $AUR_DIR"
    git -C "$AUR_DIR" fetch --quiet origin || :
    # An empty AUR remote has no refs yet — only sync if there's an
    # upstream branch to sync with. The first push creates it.
    if git -C "$AUR_DIR" rev-parse --abbrev-ref --symbolic-full-name '@{u}' \
            >/dev/null 2>&1; then
        git -C "$AUR_DIR" reset --hard --quiet '@{u}' 2>/dev/null \
            || git -C "$AUR_DIR" pull --ff-only --quiet
    fi
fi

# 2. Copy the canonical files in. The README ships as plain README.md on the
#    AUR side so forge web UIs render it as the package landing page.
cp "$SRC_PKGBUILD" "$AUR_DIR/PKGBUILD"
cp "$SRC_INSTALL"  "$AUR_DIR/usbooty-git.install"
cp "$SRC_README"   "$AUR_DIR/README.md"

# 3. Regenerate .SRCINFO (the AUR enforces this).
echo "==> Generating .SRCINFO"
( cd "$AUR_DIR" && makepkg --printsrcinfo > .SRCINFO )

# 4. Show what changed.
echo "==> Diff in $AUR_DIR:"
git -C "$AUR_DIR" status --short
echo
git -C "$AUR_DIR" --no-pager diff --stat
echo

if [ "$DO_PUSH" -eq 0 ]; then
    cat <<EOF
==> Working copy ready at: $AUR_DIR
==> Review the diff above, then either:
      (cd $AUR_DIR && git add -A && git commit -m '...' && git push)
==> or rerun this script with --push to do it automatically.
EOF
    exit 0
fi

# 5. Stage everything (modified + untracked + deletions). `git diff --quiet`
#    alone misses untracked files, which is exactly the first-push case.
git -C "$AUR_DIR" add -A
if git -C "$AUR_DIR" diff --cached --quiet; then
    echo "==> No changes to push."
    exit 0
fi

# 6. Commit + push using the upstream short-SHA in the message.
UPSTREAM_SHA="$(git -C "$PACKAGING_DIR/.." rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
MSG="Sync with upstream $UPSTREAM_SHA"
echo "==> Committing: $MSG"
git -C "$AUR_DIR" commit -m "$MSG"
echo "==> Pushing to $AUR_REMOTE"
git -C "$AUR_DIR" push
