# Installation

## From the AUR (Arch, Manjaro, EndeavourOS, etc.)

The AUR clone is a separate git repository hosted at
<https://git.thoxy.xyz/AUR/usbooty-git>. Clone it and build with `makepkg`:

```sh
git clone https://git.thoxy.xyz/AUR/usbooty-git.git
cd usbooty-git
makepkg -fsi
```

`makepkg -fsi` cleans any previous build (`-f`), pulls the build and runtime
dependencies (`-s`), and installs the resulting package (`-i`). The build
will pull a fresh clone of the upstream source repo on every run, so the
package always tracks the latest commit on `main`.

The PKGBUILD disables LTO (see the long-form note in `packaging/PKGBUILD`):
cxx-qt-lib's C++ glue compiles to GCC LTO bitcode under makepkg's default
`lto` option, which rust-lld cannot resolve at link time. If you maintain a
downstream package, keep `options=('!lto')` in your PKGBUILD.

## From source

### Prerequisites

* Rust 1.87 or newer (the workspace pins this).
* A C++ toolchain (gcc or clang).
* CMake.
* Qt 6 development packages: Qt Base, Qt Declarative, Qt Quick Controls, Qt
  SVG.

On Arch:

```sh
sudo pacman -S --needed rust cmake qt6-base qt6-declarative qt6-svg
```

On Debian or Ubuntu:

```sh
sudo apt install rustc cargo cmake \
    qt6-base-dev qt6-declarative-dev qt6-svg-dev qml6-module-qtquick-controls
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

After install, the desktop entry shows up in the applications menu, the
window icon appears in the taskbar (Wayland uses the desktop file to look it
up), and `pkexec` is wired to the polkit policy.

## Runtime dependencies

usbooty shows a banner at startup if any of these tools are missing. The
helper falls back gracefully where it can.

| Tool             | Package (Arch)   | Needed for                          |
|------------------|------------------|-------------------------------------|
| `pkexec`         | `polkit`         | Privilege escalation (required)     |
| `mkfs.vfat`      | `dosfstools`     | FAT32 formatting                    |
| `mkfs.ntfs`      | `ntfs-3g`        | NTFS formatting, UEFI:NTFS mode     |
| `mkfs.exfat`     | `exfatprogs`     | exFAT formatting                    |
| `mkfs.ext4`      | `e2fsprogs`      | ext4 partitions, persistence        |
| `wimlib-imagex`  | `wimlib`         | Splitting `install.wim`             |
| `ventoy`         | `ventoy-bin`     | The Ventoy method                   |

## Uninstall

If you installed from source via `install.sh`, remove the files it placed:

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
