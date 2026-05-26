# Linux ISOs

USBooty classifies a Linux ISO when it sees a recognisable boot
configuration under `isolinux/`, `boot/grub/`, or a known
distro-specific marker. The classification gates which features the
GUI offers (the DD method, persistence, etc.).

## ISO classification

`usbooty_core::iso_report::IsoReport::os_kind` returns
`OsKind::Linux` when:

* The ISO is bootable (has an El Torito catalog), AND
* It has either `isolinux/isolinux.cfg`, `syslinux.cfg`, or
  `boot/grub/grub.cfg`, AND
* It is not flagged as a Windows ISO.

Windows ISOs are detected separately via `sources/install.wim` (or
`install.esd`) and take precedence. A hybrid ISO that contains both
gets classified as Windows.

Detected distribution families surface as a short chip under the ISO
summary (Debian, Ubuntu, Linux Mint, Pop!_OS, Kali, Parrot, Tails,
Slax, Manjaro, Fedora, openSUSE, Arch, etc.). The family controls
which post-copy quirk fixes are applied and which persistence
strategy is offered.

## Persistent live USBs

When a partitioned write is selected and the ISO is from a live-system
family that USBooty knows how to persist, the **Persistent storage**
slider appears. You set a size (0 means off, up to 32 GiB in the
slider's range). The persistence strategy depends on the
distribution:

### Partition-based persistence

USBooty adds a second ext4 partition after the main FAT32 partition,
labelled and configured as the live system expects:

* **Debian live and derivatives**: writes a `persistence.conf` file
  at the ext4 root containing `/ union`. Debian's `live-config`
  picks this up automatically; the partition label `persistence` is
  what the live system searches for at boot.
* **Ubuntu / casper-based ISOs** (Ubuntu, Linux Mint, Pop!_OS,
  Kubuntu, Lubuntu, etc.): appends the `persistent` kernel parameter
  to the casper boot lines in the on-USB boot config
  (`isolinux/grub.cfg`, etc.) so casper looks for a `casper-rw` or
  `writable` partition. The persistence partition is labelled
  accordingly.
* **Kali**: same family as Debian live; uses `persistence.conf`.
* **Parrot**: same family as Debian live.

### Inline persistence

A few distros store persistence inside the live ISO's existing data
partition rather than a separate partition:

* **Slax**: USBooty creates the `slax/changes/` directory on the
  main partition. Slax mounts changes from there automatically. The
  persistence slider is hidden because the partition itself absorbs
  writes.

## Per-distro fix table

The helper applies a small per-distro fix table after the file copy
to paper over upstream quirks. Examples:

* **Manjaro**: rewrites `efi_boot_img` paths in the GRUB config so
  the resulting USB boots on UEFI without falling back to BIOS.
* **Tails**: relaxes signature checks that assume the original
  media was a freshly written DVD.
* **Ubuntu LTS point releases**: a known casper bug that uses a
  hard-coded UUID gets patched to the actual filesystem UUID.

The fix table lives in `crates/helper/src/distro_fixes.rs`; each
entry has a short rationale comment.

## What is not supported

* Arch, Fedora, openSUSE persistence: there is no upstream-standard
  partition persistence scheme for these. Use a DD image and accept
  that the drive is read-only, or use Ventoy with a persistence
  plugin.
* `nomodeset` and other kernel-parameter tweaks: not exposed in the
  UI. If you need them, edit the boot config on the resulting USB
  by hand.

The persistence section of the UI hides itself unless the method is
partitioned and the detected distro supports persistence.

## DD vs partitioned for Linux

DD is usually the right answer for Linux ISOs:

* Most modern Linux ISOs are isohybrid and ship with a tested boot
  loader.
* DD preserves checksums end-to-end. Re-hashing the written drive
  matches the ISO's published SHA-256, because we wrote it byte for
  byte.
* The verify pass is a clean correctness check.

Use the partitioned method when you want to keep writing to the drive
afterwards, or when you want persistence. If the distribution offers
an installer ISO that explicitly recommends a USB writer, follow the
upstream docs and use DD.
