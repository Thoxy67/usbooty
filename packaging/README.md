# Packaging usbooty

usbooty ships in two packaging formats:

| format         | scope                                 | location                       |
|----------------|---------------------------------------|--------------------------------|
| **Arch AUR**   | tightest distro integration           | [`PKGBUILD`](./PKGBUILD)       |
| **AppImage**   | universal Linux, portable single-file | [`appimage/`](./appimage/)     |

## Which one should I use?

* **AUR**: best on Arch / CachyOS / Manjaro. The polkit policy
  is installed system-wide, optdepends pull in the formatters automatically
  on demand, and `pkexec` integration is native. `paru -S usbooty-git`.
* **AppImage**: best for "I just want to try it" without installing
  anything system-wide, or on distros that ship outdated Qt (Debian stable,
  RHEL). Single file, double-click to run. The host still needs the
  filesystem tools installed.

## Runtime dependency matrix

The GUI surfaces a banner when one of the **mandatory** tools is missing
and an "optional tools missing" hint when one of the optionals is.

| binary           | provides feature                            | scope     |
|------------------|---------------------------------------------|-----------|
| `pkexec`         | privileged helper invocation                | mandatory |
| `mkfs.vfat`      | FAT32 formatting                            | optional  |
| `mkfs.ntfs`      | NTFS formatting                             | optional  |
| `mkfs.exfat`     | exFAT formatting                            | optional  |
| `mkfs.ext4`      | ext4 formatting (Linux persistence)         | optional  |
| `ventoy`         | Ventoy multi-boot USB install               | optional  |
| `wimlib-imagex`  | split install.wim for Windows on FAT32      | optional  |
| `syslinux`       | legacy-BIOS bootloader install              | optional  |
| `xorriso`        | advanced ISO inspection                     | optional  |
| `smartctl`       | SMART health probe                          | nice-to-have |
| `udisksctl`      | auto-mount Ventoy data partition            | nice-to-have |
| `xdg-open`       | open mounted partition in file manager      | nice-to-have |
| `notify-send`    | desktop notification on job finish          | nice-to-have |
| `lsblk` / `udevadm` / `mount` / `losetup` / `blkid` / `findmnt` | helper plumbing | mandatory (in util-linux + systemd) |

The "nice-to-have" tier is silently skipped when missing (no banner), since
the underlying feature also degrades silently and an extra dep banner for
each would be noisy.
