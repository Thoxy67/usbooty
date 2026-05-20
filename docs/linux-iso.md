# Linux ISOs

usbooty classifies a Linux ISO when it sees a recognisable boot configuration
under `isolinux/` or `boot/grub/`. The classification gates which features
the GUI offers (the DD method, persistence, etc.).

## ISO classification

`usbooty_core::iso_report::IsoReport::os_kind` returns `OsKind::Linux` when:

* The ISO is bootable (has an El Torito catalog), AND
* It has either `isolinux/isolinux.cfg`, `syslinux.cfg`, or
  `boot/grub/grub.cfg`, AND
* It is not flagged as a Windows ISO.

Windows ISOs are detected separately via `sources/install.wim` (or
`install.esd`) and take precedence. A hybrid ISO that contains both gets
classified as Windows.

## Persistent live USBs

When a partitioned write is selected and the ISO is a Debian or Ubuntu
family live system, usbooty offers a **Persistent storage** slider. You set
a size (0 means off, up to 32 GiB in the slider's range), and usbooty adds
a second ext4 partition labelled `persistence` after the main FAT32
partition.

The setup that goes on it depends on the detected distribution:

* **Debian live and derivatives**: writes a `persistence.conf` file at the
  ext4 root containing `/ union`. Debian's `live-config` picks this up
  automatically; the partition label `persistence` is what the live system
  searches for at boot.
* **Ubuntu / casper-based ISOs**: appends the `persistent` kernel parameter
  to the casper boot lines in the on-USB boot config (`isolinux/grub.cfg`,
  etc.) so casper looks for a `casper-rw` or `writable` partition. The
  persistence partition is labelled accordingly.

## What is not supported

* Arch, Fedora, openSUSE: there is no upstream-standard partition
  persistence scheme for these. Use a DD image and accept that the drive is
  read-only, or use Ventoy with a persistence plugin.
* `nomodeset` and other kernel-parameter tweaks: not exposed in the UI. If
  you need them, edit the boot config on the resulting USB by hand.

The persistence section of the UI hides itself unless both conditions hold:
the method is partitioned, and `persistence_supported` is true for the
detected distribution.

## DD vs partitioned for Linux

DD is usually the right answer for Linux ISOs:

* Most modern Linux ISOs are isohybrid and ship with a tested boot loader.
* DD preserves checksums end-to-end. Re-hashing the written drive matches
  the ISO's published SHA-256, because we wrote it byte for byte.
* The verify pass is a clean correctness check.

Use the partitioned method when you want to keep writing to the drive
afterwards, or when you want persistence. If the distribution offers an
installer ISO that explicitly recommends a USB writer, follow the upstream
docs and use DD.
