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
summary: Ubuntu, Linux Mint, LMDE, Debian, Fedora, Bazzite, Nobara,
AlmaLinux, Rocky Linux, CentOS Stream, openSUSE, GeckoLinux, Arch Linux,
Manjaro, EndeavourOS, CachyOS, Alpine Linux, Slax, and Knoppix.
Detection is most-specific-first (a derivative like Bazzite or LMDE wins
over its parent) by ISO volume label, with a `slax/` or `knoppix*` root
directory overriding the label, then a structural fallback (`casper/`
becomes Ubuntu, `live/` becomes Debian, `LiveOS/` becomes the
Fedora / RHEL family, `arch/` becomes Arch). Distros without their own
label needle (Kali, Parrot, Pop!_OS, and similar) fall through to that
structural check and are handled as their Debian or Ubuntu base. The
family controls which post-copy quirk fixes are applied and which
persistence strategy is offered.

## Persistent live USBs

When a partitioned write is selected and the ISO is from a live-system
family that USBooty knows how to persist, the **Persistent storage**
slider appears. You set a size (0 means off); the slider's maximum is
whatever space is left on the device after the ISO and a small
partition-table margin. The persistence strategy depends on the
distribution:

### Partition-based persistence

USBooty adds a second ext4 partition after the main partition,
labelled and configured the way each live system expects:

* **Ubuntu / casper-based** (Ubuntu, Linux Mint, Kubuntu, Lubuntu,
  Pop!_OS, etc.): partition labelled `casper-rw`, and the
  `persistent` keyword is appended to the `boot=casper` line in the
  on-USB boot configs so casper looks for it.
* **Debian live** (Debian, LMDE, Kali, Parrot, etc.): partition
  labelled `persistence` carrying a `persistence.conf` file with
  `/ union`, plus `persistence` appended to the `boot=live` line.
  `live-config` picks this up automatically.
* **Fedora / RHEL family** (Fedora, Bazzite, Nobara, AlmaLinux,
  Rocky Linux, CentOS Stream): an ext4 partition labelled `OVERLAY`
  holding a sparse COW file `overlay.img`. dracut's `dmsquash-live`
  loop-mounts that file as a dm-snapshot when
  `rd.live.overlay=LABEL=OVERLAY:/overlay.img` is on the kernel command
  line (a bare partition is not enough). These distros all share the
  same dracut live stack. **Verify on hardware:** the COW wiring is
  unit-tested, but reboot persistence has not been bench-tested.
* **openSUSE** (openSUSE, GeckoLinux): USBooty adds
  `rd.live.overlay.persistent` to the kernel command line. **Known
  limitation:** kiwi-live creates its own write partition in
  unpartitioned free space rather than adopting a labelled one, so full
  persistence also needs the device left with free space, which the
  current layout does not do yet. Treat openSUSE persistence as
  experimental.
* **Arch / archiso** (Arch, Manjaro, EndeavourOS, CachyOS): an ext4
  partition labelled `PERSISTENCE`, activated by appending
  `cow_label=PERSISTENCE` to the kernel command line.

### Inline persistence

A few distros store persistence inside the live ISO's existing data
partition rather than a separate partition:

* **Slax**: USBooty creates the `slax/changes/` directory on the
  main partition. Slax saves changes there automatically on writable
  media (no kernel parameter needed). The persistence slider is hidden
  because the partition itself absorbs writes.
* **Alpine Linux** (diskless mode): Alpine does not use an overlay
  partition at all. It runs from RAM and persists configuration with
  `lbu`, which writes an `<host>.apkovl.tar.gz` to any writable
  filesystem on the boot media. Write Alpine with the **Partition &
  copy** method onto a writable FAT32/ext4 stick, then run `lbu commit`
  inside the running system. USBooty shows no persistence slider for
  Alpine.

## Per-distro fix table

The helper applies a small per-distro fix table after the file copy
to paper over upstream quirks. Every fix is idempotent and non-fatal.
The fixes implemented today:

* **Arch / Manjaro / EndeavourOS / CachyOS / Bazzite / GeckoLinux**:
  several of these ship a signed `grubx64.efi` with a hard-coded
  `prefix=` that breaks once the ISO is copied onto a USB with a
  different layout. USBooty writes a fallback `/EFI/BOOT/grub.cfg`
  that locates the real config by volume label (`search --label`)
  and chainloads it, but only when the ISO does not already ship
  one.
* **Knoppix**: appends `vga=normal nodma` to the isolinux boot
  entries (matching Knoppix's own failsafe defaults), because older
  releases hang on some GPUs in their default vesa mode.

The fix table lives in `crates/helper/src/distro_fixes.rs`; each
entry has a short rationale comment.

## What is not supported

* Persistence for ISOs USBooty does not recognise as a known live
  family (and for Knoppix): no overlay partition is offered. Use a
  DD image, or Ventoy with a persistence plugin.
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
