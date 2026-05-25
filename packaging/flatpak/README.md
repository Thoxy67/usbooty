# Flatpak packaging

Out-of-the-box this directory builds usbooty against the **KDE 6.7** platform
runtime (the cheapest way to get Qt 6 and the QML runtime without rebuilding
them ourselves).

## Build

```sh
# Generate cargo-sources.json so the Flatpak sandbox can build offline.
# Re-run after every Cargo.lock change.
pip install --user toml requests  # one-time
python3 \
  $(curl -sL https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py) \
  ../../Cargo.lock -o cargo-sources.json

# Install KDE 6.7 SDK + the Rust extension on flathub.
flatpak install --user flathub \
  org.kde.Platform//6.7 \
  org.kde.Sdk//6.7 \
  org.freedesktop.Sdk.Extension.rust-stable//23.08

# Build into a local repo.
flatpak-builder --force-clean --user \
  --install --install-deps-from=flathub \
  build-dir org.usbooty.Usbooty.yml
```

After install:

```sh
flatpak run org.usbooty.Usbooty
```

## Caveats vs. the native package

A Flatpak sandbox is the wrong shape for a tool that writes raw block
devices. The manifest punches the necessary holes (`--device=all`,
`--socket=system-bus`, polkit on the system bus), but a few host-side
optional dependencies are **not** carried in the runtime and must exist on
the host system itself for the corresponding features to work:

| feature                                  | host requirement                |
|------------------------------------------|---------------------------------|
| FAT32 / NTFS / exFAT / ext4 format       | `dosfstools`/`ntfs-3g`/`exfatprogs`/`e2fsprogs` |
| Ventoy install                           | `ventoy` (host install)         |
| Split install.wim onto FAT32             | `wimlib-imagex`                 |
| Syslinux MBR install for legacy BIOS     | `syslinux`                      |
| SMART health probe in Inspect panel      | `smartmontools`                 |
| Auto-mount + open Ventoy data partition  | `udisks2`, `xdg-utils`          |
| Desktop notification on job completion   | `libnotify`                     |

A Flatpak install on a Steam-Deck-style minimal host will boot the GUI and
hash an ISO, but cannot format or install Ventoy until the missing host
tools are installed. The dependency banner in the GUI surfaces which
specific tool is missing.

## Why not bundle everything?

Bundling `mkfs.*` / `ventoy` / `syslinux` inside the Flatpak would solve
the "missing host tool" problem but introduces a worse one: the bundled
filesystem tools couldn't see /dev/sdX from inside the sandbox without
elevating the whole Flatpak to a permission level that defeats the
purpose. usbooty's privilege boundary is the `usbooty-helper` binary
launched via pkexec; running it from inside the sandbox via the host's
polkit is the cleanest split.
