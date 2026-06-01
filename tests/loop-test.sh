#!/bin/sh
# Integration test for usbooty-helper against a loopback image; no real USB
# drive is touched. Requires root (losetup, mkfs, mount) and xorriso.
#
# Usage: sudo ./tests/loop-test.sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "This test must be run as root (losetup/mkfs/mount need it)." >&2
    exit 1
fi

SRC_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HELPER="$SRC_DIR/target/release/usbooty-helper"
[ -x "$HELPER" ] || HELPER="$SRC_DIR/target/debug/usbooty-helper"
[ -x "$HELPER" ] || { echo "Build the helper first: cargo build" >&2; exit 1; }

WORK="$(mktemp -d)"
LOOP=""
cleanup() {
    [ -n "$LOOP" ] && losetup -d "$LOOP" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

# --- Build a small test ISO ------------------------------------------------
mkdir -p "$WORK/iso/boot"
echo "usbooty loop-test marker" > "$WORK/iso/readme.txt"
head -c 1048576 /dev/urandom > "$WORK/iso/boot/data.bin"
xorriso -as mkisofs -J -R -o "$WORK/test.iso" "$WORK/iso" >/dev/null 2>&1

# --- Create a 256 MiB loopback "device" -----------------------------------
truncate -s 256M "$WORK/disk.img"
LOOP="$(losetup --find --show --partscan "$WORK/disk.img")"
echo "Loop device: $LOOP"

# Feed a job to the helper, keeping its stdin open (via a fifo) for the whole
# run so the EOF-means-cancel watcher does not fire prematurely.
run_job() {
    fifo="$WORK/ctl"
    rm -f "$fifo"; mkfifo "$fifo"
    exec 3<>"$fifo"
    printf '%s\n' "$1" >&3
    "$HELPER" < "$fifo"
    exec 3>&-
}

echo
echo "=== DD method ==="
run_job "{\"kind\":\"dd\",\"iso_path\":\"$WORK/test.iso\",\"device_path\":\"$LOOP\"}"
iso_size=$(stat -c %s "$WORK/test.iso")
if head -c "$iso_size" "$LOOP" | cmp -s - "$WORK/test.iso"; then
    echo "DD verify: OK"
else
    echo "DD verify: FAILED" >&2; exit 1
fi

echo
echo "=== FAT32 method (GPT) ==="
run_job "{\"kind\":\"partitioned\",\"iso_path\":\"$WORK/test.iso\",\"device_path\":\"$LOOP\",\"table\":\"gpt\",\"wim_strategy\":\"none\",\"uefi_ntfs_img\":null}"
partprobe "$LOOP" 2>/dev/null || true
sleep 1
mkdir -p "$WORK/mnt"
mount "${LOOP}p1" "$WORK/mnt"
if [ -f "$WORK/mnt/readme.txt" ] && grep -q "loop-test marker" "$WORK/mnt/readme.txt"; then
    echo "FAT32 verify: OK"
    ok=0
else
    echo "FAT32 verify: FAILED" >&2; ok=1
fi
umount "$WORK/mnt"
[ "$ok" -eq 0 ] || exit 1

echo
echo "loop-test completed successfully."
