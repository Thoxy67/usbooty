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
summary: Ubuntu, Kubuntu/Xubuntu/Lubuntu, Linux Mint, LMDE, Pop!_OS,
Zorin OS, elementary OS, KDE neon, Linux Lite, Debian, Kali Linux,
Tails, Fedora, Bazzite, Nobara, AlmaLinux, Rocky Linux, CentOS Stream,
openSUSE, GeckoLinux, Arch Linux, Manjaro, EndeavourOS, CachyOS,
Garuda, Artix, Alpine Linux, Slax, Knoppix, Puppy, and antiX / MX
Linux. Detection is most-specific-first (a derivative like Bazzite or
LMDE wins over its parent) by ISO volume label, with a root-directory
marker overriding the label (`slax/`, `knoppix*`, `antiX/`, or a
`puppy_*.sfs` file), then a structural fallback (`casper/` becomes
Ubuntu, `live/` becomes Debian, `LiveOS/` becomes the Fedora / RHEL
family, `arch/` becomes Arch). Distros without their own label needle
(Parrot and similar) fall through to that structural check and are
handled as their Debian or Ubuntu base. The family controls which
post-copy quirk fixes are applied and which persistence strategy is
offered.

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

* **Ubuntu / casper-based** (Ubuntu, Kubuntu, Lubuntu, Xubuntu,
  Linux Mint, Pop!_OS, Zorin OS, elementary OS, KDE neon, Linux
  Lite): partition labelled `casper-rw`, and the `persistent`
  keyword is appended to the casper boot entries in the on-USB boot
  configs. The patcher matches several anchors per file (`boot=casper`,
  `file=/cdrom/preseed`, and the `/casper/vmlinuz` kernel lines), so
  GRUB-only ISOs such as Ubuntu 23.04+ that no longer spell
  `boot=casper` still get patched.
* **Debian live** (Debian, LMDE, Kali, Parrot, etc.): partition
  labelled `persistence` carrying a `persistence.conf` file with
  `/ union`, plus `persistence` appended to the `boot=live` line.
  `live-config` picks this up automatically. Kali also has a native
  "Live USB Persistence" boot-menu entry that uses the same partition.
* **Fedora 40+ and atomic spins** (Fedora, Bazzite, Nobara): an ext4
  partition labelled `OVERLAY` used directly as an overlayfs upper
  layer via `rd.live.overlay=LABEL=OVERLAY
  rd.live.overlay.overlayfs=1`. No fixed-size COW file, so the
  overlay cannot exhaust itself the way a dm-snapshot can.
  **Verify on hardware:** the wiring is unit-tested, but reboot
  persistence has not been bench-tested.
* **RHEL rebuilds** (AlmaLinux, Rocky Linux, CentOS Stream): same
  `OVERLAY` partition but holding a sparse COW file `overlay.img`,
  loop-mounted as a dm-snapshot via
  `rd.live.overlay=LABEL=OVERLAY:/overlay.img`. Their older dracut
  stacks predate the overlayfs mode. Same bench-test caveat.
* **openSUSE** (openSUSE, GeckoLinux): an ext4 partition labelled
  `cow` plus `rd.live.overlay.persistent` on the kernel command
  line. **Known limitation:** kiwi-live prefers creating its own
  write partition in unpartitioned free space rather than adopting a
  labelled one; treat openSUSE persistence as experimental.
* **Arch / archiso and forks** (Arch, Manjaro, EndeavourOS, CachyOS,
  Garuda, Artix): an ext4 partition labelled `PERSISTENCE`,
  activated by appending `cow_label=PERSISTENCE` to the kernel
  command line. The patcher anchors on `archisobasedir=` (archiso)
  and `misobasedir=` (Manjaro's miso fork) alike.
* **Knoppix**: an ext4 partition labelled `KNOPPIX-DATA`. The
  Knoppix initrd auto-scans for that label at boot, so no boot
  config is patched at all.

If a partition-based scheme ends up patching zero boot config files
(an ISO whose layout USBooty has never seen), the job logs a loud
warning: the partition exists but the live system will not look for
it, so persistence is effectively off.

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

### Distros that manage persistence themselves

For these, USBooty shows a short explanatory note instead of the
slider:

* **Tails** creates its own LUKS-encrypted "Persistent Storage" from
  inside the running system, and refuses media it did not lay out
  itself. Write Tails with the **DD** method and configure
  persistence in Tails' welcome screen.
* **Puppy Linux** offers to create its save file or folder on first
  shutdown; nothing needs preparing in advance.
* **antiX / MX Linux** configure persistence from their own live
  boot menu (`persist_root` / `persist_home`), which creates the
  rootfs/homefs files on the writable partition on demand.

## Per-distro fix table

The helper applies a small per-distro fix table after the file copy
to paper over upstream quirks. Every fix is idempotent and non-fatal.
The fixes implemented today:

* **Arch / Manjaro / EndeavourOS / CachyOS / Garuda / Artix /
  Bazzite / GeckoLinux**:
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
  family: no overlay partition is offered. Use a DD image, or
  Ventoy with a persistence plugin.
* PCLinuxOS has no standard documented persistence mechanism;
  LUKS-encrypted Kali persistence is not wired up (the plain
  partition is).
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

Either way, you can boot the finished stick in a VM without rebooting
your machine: **Device → Verify boot device (QEMU)**. See
[Write methods](write-methods.md#boot-testing-the-finished-stick-qemu).
