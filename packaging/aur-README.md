# usbooty-git

Arch User Repository package for [usbooty](https://github.com/thoxy/usbooty) —
a Rufus-like tool that creates bootable USB drives from ISO images.

This is the **git** package: it builds the latest commit from upstream.

## Features

- **DD** raw image write (isohybrid ISOs, BSD memstick images, …)
- **Partition & copy** — FAT32 / NTFS / exFAT / ext4, including the Windows
  **UEFI:NTFS** layout for ISOs with a large `install.wim`
- **Windows 11 setup customization** — bypass TPM / Secure Boot / RAM checks,
  skip the Microsoft-account requirement, create a local account
- **Linux live-USB persistence** (Debian / Ubuntu family)
- **Format only** — a blank FAT32 / NTFS / exFAT / ext4 drive
- **Ventoy** multi-boot USB creation (install / update + drop an ISO)
- Built-in Windows ISO downloader, write verification, SHA-256 display

## Install

```sh
git clone https://git.thoxy.xyz/AUR/usbooty-git.git
cd usbooty-git
makepkg -si
```

## Optional dependencies

`dosfstools`, `ntfs-3g`, `exfatprogs`, `e2fsprogs` provide the respective
filesystem formatters; `ventoy` enables the Ventoy method. Install whichever
you need.

## License

MIT — see the project repository.
