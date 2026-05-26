# Installation

## From the AUR (Arch, Manjaro, EndeavourOS, etc.)

The AUR clone is a separate git repository hosted at
<https://git.thoxy.xyz/AUR/usbooty-git>. Clone it and build with
`makepkg`:

```sh
git clone https://git.thoxy.xyz/AUR/usbooty-git.git
cd usbooty-git
makepkg -fsi
```

`makepkg -fsi` cleans any previous build (`-f`), pulls the build and
runtime dependencies (`-s`), and installs the resulting package
(`-i`). The build will pull a fresh clone of the upstream source repo
on every run, so the package always tracks the latest commit on
`main`.

The PKGBUILD disables LTO (see the long-form note in
`packaging/PKGBUILD`): cxx-qt-lib's C++ glue compiles to GCC LTO
bitcode under makepkg's default `lto` option, which rust-lld cannot
resolve at link time. If you maintain a downstream package, keep
`options=('!lto')` in your PKGBUILD.

## From source

### Prerequisites

* Rust 1.87 or newer (the workspace pins this).
* A C++ toolchain (gcc or clang).
* CMake.
* Qt 6 development packages: Qt Base, Qt Declarative, Qt Quick
  Controls, Qt SVG, Qt Linguist Tools (`lupdate6`, `lrelease6` for
  translation refresh).

On Arch:

```sh
sudo pacman -S --needed rust cmake qt6-base qt6-declarative qt6-svg \
    qt6-tools
```

On Debian or Ubuntu:

```sh
sudo apt install rustc cargo cmake \
    qt6-base-dev qt6-declarative-dev qt6-svg-dev \
    qml6-module-qtquick-controls qt6-tools-dev-tools
```

### Build and install

```sh
cargo build --release
sudo ./install.sh
```

The install script places:

| File                                       | Destination                                          |
|--------------------------------------------|------------------------------------------------------|
| `target/release/usbooty`                   | `/usr/bin/usbooty`                                   |
| `target/release/usbooty-helper`            | `/usr/libexec/usbooty/usbooty-helper`                |
| `data/org.usbooty.helper.policy`           | `/usr/share/polkit-1/actions/`                       |
| `data/org.usbooty.Usbooty.desktop`         | `/usr/share/applications/`                           |
| `data/org.usbooty.Usbooty.metainfo.xml`    | `/usr/share/metainfo/`                               |
| `data/icons/org.usbooty.Usbooty.svg`       | `/usr/share/icons/hicolor/scalable/apps/`            |

After install, the desktop entry shows up in the applications menu,
the window icon appears in the taskbar (Wayland uses the desktop
file to look it up), and `pkexec` is wired to the polkit policy.

## Runtime dependencies

USBooty shows a banner at startup if any of these tools are missing.
The helper falls back gracefully where it can. Items marked
"required" must be present for the matching feature to work; items
marked "optional" are nice-to-haves.

| Tool             | Package (Arch)        | Needed for                                          | Required? |
|------------------|-----------------------|-----------------------------------------------------|-----------|
| `pkexec`         | `polkit`              | Privilege escalation.                               | Required  |
| `mkfs.vfat`      | `dosfstools`          | FAT16 / FAT32 formatting.                           | Required for FAT |
| `mkfs.ntfs`      | `ntfs-3g`             | NTFS formatting, UEFI:NTFS mode.                    | Optional  |
| `mkfs.exfat`     | `exfatprogs`          | exFAT formatting, UEFI:exFAT mode.                  | Optional  |
| `mkfs.ext4`      | `e2fsprogs`           | ext4 partitions, partition-based persistence.       | Optional  |
| `mkfs.ext2`, `.ext3`, `.udf`, `.btrfs`, `.xfs`, `.f2fs`, `.jfs`, `.nilfs2` | (matching `*-progs` packages) | Other filesystems in the picker. | Optional  |
| `wimlib-imagex`  | `wimlib`              | `WimStrategy::Split`, Windows CA 2023 extraction.   | Optional  |
| `ventoy`         | `ventoy-bin`          | The Ventoy method.                                  | Optional  |
| `mtools`         | `mtools`              | FreeDOS bootable USB (`mformat`, `mcopy`).          | Optional  |
| `smartmontools`  | `smartmontools`       | SMART probe of the selected device.                 | Optional  |
| `fuse3`          | `fuse3`               | Mounting ISOs to read file lists for the partition copy. The DD method does not need it. | Optional |
| `udisksctl` or `eject` | `udisks2` / `util-linux` | Powering off the drive cleanly after a write.  | Optional  |

USBooty does not bundle any of these tools. Missing optionals just
mean the matching features stay disabled (the filesystem combo only
lists tools that are actually installed; the SMART chip stays empty
if `smartctl` is absent; etc.).

The table above lists the tools you are most likely to need.
`packaging/PKGBUILD` ships the full `optdepends` array (every
filesystem formatter plus `libisoburn`, `xdg-utils`, `libnotify`,
etc.); install from the AUR and `pacman` offers them as part of the
package install. If you build from source on a non-Arch distro, you
can use the PKGBUILD as a checklist for the bigger optional set.

## User preferences

USBooty persists two preferences in `~/.config/usbooty/settings.json`:

* **Force English**: opts out of the locale-based French translation
  and runs the GUI in its English source language. Toggled from the
  `?` menu.
* **Always show activity log**: keeps the activity log column open
  even when the buffer is empty, instead of auto-expanding on the
  first log line. Toggled from the `?` menu.

Both preferences live-apply (no restart needed).

## Cache

Downloaded resources live under `$XDG_CACHE_HOME/usbooty/` (typically
`~/.cache/usbooty/`):

* `uefi-ntfs.img`: the Rufus EFI bootloader image used by the
  UEFI:NTFS / UEFI:exFAT strategies.
* `freedos/`: the latest FreeDOS `KERNEL.SYS`, `COMMAND.COM`, and
  boot sector binaries fetched from upstream GitHub releases. The
  resolver runs once a day.
* `dbx-x64.bin` / `dbx-arm64.bin`: the live UEFI Forum DBX revocation
  file, used by the SBAT / DBX scanner.

You can delete the cache safely; USBooty re-downloads on next use.

## Uninstall

If you installed from source via `install.sh`, remove the files it
placed:

```sh
sudo rm -f /usr/bin/usbooty \
           /usr/libexec/usbooty/usbooty-helper \
           /usr/share/polkit-1/actions/org.usbooty.helper.policy \
           /usr/share/applications/org.usbooty.Usbooty.desktop \
           /usr/share/metainfo/org.usbooty.Usbooty.metainfo.xml \
           /usr/share/icons/hicolor/scalable/apps/org.usbooty.Usbooty.svg
sudo rmdir /usr/libexec/usbooty 2>/dev/null
```

If installed from the AUR: `sudo pacman -R usbooty-git`.
