# Other and niche systems

USBooty's deepest support is for Windows and Linux installers (see
[Windows ISOs](windows-iso.md) and [Linux ISOs](linux-iso.md)), but it
writes plenty of other systems too. It sorts every loaded image into one
of four kinds and pre-selects a sensible write method:

| Kind          | How it is recognised                                  | Auto-selected method |
|---------------|-------------------------------------------------------|----------------------|
| Windows       | `sources/install.wim` (or `.esd`) plus `bootmgr` / `setup.exe` | Partition & copy |
| Linux         | an `isolinux/` or `boot/grub*` boot config            | Partition & copy     |
| BSD           | `bsd` in the ISO volume label                         | DD (raw image)       |
| Generic image | anything else                                         | DD (raw image)       |

You can always override the method in the Options card.

## BSD (FreeBSD, OpenBSD, NetBSD, DragonFly)

BSD install images are isohybrid: the published `.iso` already carries a
working boot sector and partition table. The right move is **DD**, a raw
byte-for-byte copy, which USBooty pre-selects when it sees `bsd` in the
volume label. No partitioning, persistence, or per-distro fixes apply.
If a BSD image is labelled in a way USBooty does not recognise, set the
method to DD by hand; the result is identical.

## FreeDOS

FreeDOS is a build target, not an ISO to load. Pick the **FreeDOS
bootable USB** write method and USBooty assembles a self-contained DOS
stick with no ISO at all: it pulls the latest `KERNEL.SYS` and
`COMMAND.COM` from the upstream FreeDOS GitHub releases (cached locally),
formats the drive FAT16 or FAT32, writes the FreeDOS boot sector with
`mformat -B` plus a Syslinux MBR, and copies the kernel and shell in.
Needs `mtools` on the host. The stick stays fully writable, so you can
drop your own BIOS/UEFI flashing utilities and legacy DOS tools onto it.
Details in [Write methods](write-methods.md).

## ReactOS and everything else

ReactOS, Haiku, KolibriOS, memtest86, rescue / antivirus tools, router
and NAS images, and similar niche systems are handled as a **Generic
image**: USBooty makes no assumptions and offers **DD** by default.

* If the image is isohybrid (most modern downloadable `.iso` / `.img`
  files are), DD it and it boots the way the publisher intended.
* If you instead want a writable FAT32 / exFAT data area on the stick,
  switch to **Partition & copy** and the files land on a fresh
  filesystem.

There is no per-OS quirk handling for these, and no persistence is
offered. A plain CD image that was never made USB-bootable (some older
ReactOS and rescue CDs) may not boot when DD'd; in that case follow the
project's own USB-creation instructions. Compressed inputs (`.xz`,
`.gz`, `.bz2`, `.zst`, `.lzma`, `.zip`, `.Z`) and fixed `.vhd` images are
unpacked transparently before writing, whatever the system inside.
