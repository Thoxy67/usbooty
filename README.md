# usbooty

Create bootable USB drives from ISO images on Linux. Rust with a Qt 6 / QML
front end, and the bootable-media logic ported from
[Rufus](https://github.com/pbatard/rufus).

## What it does

* Raw DD, partitioned copy (FAT32 / NTFS / exFAT / ext4), plain format, and
  Ventoy multi-boot installs.
* Windows install media with the UEFI:NTFS dual-partition layout for ISOs
  whose `install.wim` is larger than the FAT32 4 GiB file limit.
* Windows 11 setup customisation via `autounattend.xml`: hardware-check
  bypass, local account, locale, time zone, and a debloat profile.
* Direct Windows 11 ISO download from Microsoft (a port of Rufus's Fido).
* Persistent overlay partitions for Debian and Ubuntu family live USBs.

## Quick install

From the AUR (Arch and derivatives):

```sh
git clone https://git.thoxy.xyz/AUR/usbooty-git.git
cd usbooty-git
makepkg -fsi
```

From source:

```sh
cargo build --release
sudo ./install.sh
```

## Documentation

See [`docs/`](docs/) for the full picture. Start with
[`docs/index.md`](docs/index.md) for the table of contents.

## License

GPL-3.0-or-later.
