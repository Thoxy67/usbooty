#!/bin/sh
# Round-trip test for the GUI's decompression cache. For each supported
# algorithm we:
#   1. compress a known-good ISO,
#   2. invoke the GUI as `usbooty --iso <compressed> --help-after-cache`
#      (a no-op short-circuit added only for this test path... not implemented;
#      instead we drive the library directly by running a small Rust example
#      via `cargo run -p usbooty-gui --example decompress_smoke`).
#
# Today this script is the integration scaffolding around the unit-test fixture
# expected at `crates/gui/tests/decompress_smoke.rs`. It just verifies the
# external compression tools are installed and writes a fresh ISO that the
# Rust-side test consumes via fixture path.
#
# Usage: ./tests/decompress-test.sh

set -eu

SRC_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$SRC_DIR/target/decompress-test"
mkdir -p "$WORK"

ISO="$WORK/test.iso"

# Generate a tiny ISO once; the seed is constant so the cache hash is stable
# across runs and the .meta.json sidecar can be inspected by eye.
if [ ! -s "$ISO" ]; then
    if ! command -v xorriso >/dev/null 2>&1; then
        echo "xorriso not installed — install xorriso (libisoburn) and re-run." >&2
        exit 1
    fi
    rm -rf "$WORK/seed"
    mkdir -p "$WORK/seed"
    printf 'usbooty decompress-test marker\n' > "$WORK/seed/marker.txt"
    head -c 524288 /dev/urandom > "$WORK/seed/random.bin"
    xorriso -as mkisofs -J -R -o "$ISO" "$WORK/seed" >/dev/null 2>&1
    echo "Built $ISO ($(stat -c %s "$ISO") bytes)"
fi

# Build one compressed fixture, skipping any tool that isn't installed — the
# decompressor still has to handle whichever ones the CI host happens to have.
# (Named `make_fixture` so it doesn't shadow the `compress(1)` binary the .Z
# arm needs to invoke.)
make_fixture() {
    tool=$1; ext=$2; cmd=$3
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "  $tool: missing, skipping"
        return
    fi
    out="$WORK/test.iso.$ext"
    rm -f "$out"
    # shellcheck disable=SC2086
    eval "$cmd" < "$ISO" > "$out"
    echo "  $tool: $(stat -c %s "$out") bytes -> $out"
}

echo "=== Compressing fixtures ==="
make_fixture xz       xz   'xz -c -T0'
make_fixture gzip     gz   'gzip -c'
make_fixture bzip2    bz2  'bzip2 -c'
make_fixture zstd     zst  'zstd -q'
make_fixture xz       lzma 'xz --format=lzma -c'
# Unix compress(1). Provided by the `ncompress` package on most distros
# (Arch: `ncompress`, Debian: `ncompress`). If absent, the fixture is
# skipped and the Rust-side `.Z` test soft-passes via the same shape the
# other unavailable tools use.
make_fixture compress Z    'compress -c'
if command -v zip >/dev/null 2>&1; then
    rm -f "$WORK/test.iso.zip"
    (cd "$WORK" && zip -q test.iso.zip test.iso) || true
    [ -f "$WORK/test.iso.zip" ] && echo "  zip: $(stat -c %s "$WORK/test.iso.zip") bytes"
else
    echo "  zip: missing, skipping"
fi

echo
echo "=== Library smoke (cargo test) ==="
cd "$SRC_DIR"
USBOOTY_DECOMPRESS_FIXTURE_DIR="$WORK" \
    cargo test -p usbooty-gui --test decompress_smoke -- --include-ignored

echo
echo "decompress-test completed."
