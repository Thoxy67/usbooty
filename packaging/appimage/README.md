# AppImage packaging

A single-file portable build that carries the Qt 6 runtime + QML modules
so it runs on any glibc-2.31+ host (Ubuntu 20.04 LTS / Debian 12 / Fedora
36+ / Arch).

## Build

```sh
./packaging/appimage/build-appimage.sh
```

Outputs `usbooty-x86_64.AppImage` in the repo root.

## Host requirements at runtime

The AppImage ships Qt 6 + QML + the usbooty binaries, but the
**host operating system still needs the optional CLI tools** that the
helper shells out to:

| feature                                  | host package                    |
|------------------------------------------|---------------------------------|
| pkexec (mandatory)                       | `polkit`                        |
| FAT32 format                             | `dosfstools`                    |
| NTFS format                              | `ntfs-3g`                       |
| exFAT format                             | `exfatprogs`                    |
| ext4 format                              | `e2fsprogs`                     |
| Ventoy install                           | `ventoy`                        |
| Split install.wim onto FAT32             | `wimlib` (`wimlib-imagex`)      |
| Windows To Go (apply image + BCD)        | `wimlib`, `hivex`, `ntfs-3g`    |
| Syslinux MBR install for legacy BIOS     | `syslinux`                      |
| SMART probe in Inspect panel             | `smartmontools`                 |
| Auto-mount + open Ventoy data partition  | `udisks2`, `xdg-utils`          |
| Desktop notification on long jobs        | `libnotify` (`notify-send`)     |

The dependency banner in the GUI surfaces which specific tool is missing
on the host.

## Why an AppImage isn't fully self-contained

A USB writer's whole job is to talk to the kernel about block devices,
which means it has to run the host's `mkfs.*`, `udevadm`, `lsblk`,
`pkexec`, etc. Bundling those inside the AppImage would mean shipping a
full chroot, at which point a container is the right shape. The split
usbooty actually uses is:

* **Bundled**: the Qt runtime, the QML modules, the usbooty binaries.
* **Host**: filesystem tools, polkit, udev, optional features.

## Caveats vs. the native package

* `pkexec` integration depends on the **host's** polkit reading the policy
  file. The AppImage installs it at first launch via the freedesktop
  AppImage launch protocol on supported file managers; otherwise the
  policy is loaded ephemerally and the user gets one prompt per session
  instead of one persistent allow-rule.
* SMART probes need either a setuid `smartctl` on the host or running
  usbooty itself with sudo. Same as the native install.
