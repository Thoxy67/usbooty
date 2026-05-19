# usbooty

A Linux desktop app that turns an ISO image into a bootable USB drive. Written in
Rust with a Qt 6 / QML front-end, with the bootable-media logic ported from
[Rufus](https://github.com/pbatard/rufus).

## Features

- **Two write methods**
  - **DD image** — a raw, byte-for-byte copy of the ISO onto the device. Best for
    isohybrid Linux ISOs and disk images.
  - **FAT32 partition** — builds a partition table and filesystem and copies the ISO
    contents file by file, producing a writable drive. The user explicitly chooses
    the **GPT (UEFI)** or **MBR (BIOS/Legacy)** partition scheme.
- **Windows install media** — for a Windows ISO whose `install.wim` exceeds the
  FAT32 4 GiB single-file limit, usbooty asks how to proceed:
  - **Split** `install.wim` into `install.swm` chunks with `wimlib-imagex`, or
  - **UEFI:NTFS** — a large NTFS partition for the Windows files plus a tiny FAT32
    partition carrying the Rufus UEFI:NTFS bootloader.
- **Download Windows 11** directly from Microsoft (a native port of Rufus's "Fido"
  logic — no external scripts).
- Rufus resource files (the UEFI:NTFS image) are **downloaded at runtime** from the
  Rufus GitHub repository and cached, so the app always tracks the latest version.

## Architecture

A Cargo workspace of three crates:

| Crate | Role |
|-------|------|
| `usbooty-core` | Shared serializable types (`Job`, `ProgressMsg`, `IsoReport`, …) and the pure partition-planning logic. No Qt, no network, no root. |
| `usbooty-gui` | The unprivileged Qt/QML application (binary `usbooty`). Device enumeration, ISO analysis, networking, UI. |
| `usbooty-helper` | A small privileged CLI binary. Performs every device mutation. No GUI, no networking. |

The GUI always runs unprivileged. It builds a fully-resolved `Job`, then runs
`usbooty-helper` once via `pkexec` — a single polkit prompt per write. The helper
reads the `Job` as JSON on stdin and streams progress as JSON on stdout. Writing
`cancel` to its stdin (or closing it) aborts the job cleanly.

## Build

Requires **Rust 1.87+**, a **C++ toolchain**, **CMake**, and **Qt 6** development
packages (Qt Base + Qt Declarative / Qt Quick Controls).

```sh
cargo build --release
```

## Install

```sh
sudo ./install.sh
```

This installs:

- `usbooty` → `/usr/bin/usbooty`
- `usbooty-helper` → `/usr/libexec/usbooty/usbooty-helper`
- the polkit policy → `/usr/share/polkit-1/actions/`
- the desktop entry and AppStream metadata → `/usr/share/`

## Runtime dependencies

| Tool | Package | Needed for |
|------|---------|------------|
| `pkexec` | polkit | privilege escalation (required) |
| `mkfs.vfat` | dosfstools | FAT32 formatting |
| `mkfs.ntfs` | ntfs-3g | UEFI:NTFS mode |
| `wimlib-imagex` | wimlib / wimtools | `install.wim` splitting |

usbooty shows a banner when a needed tool is missing.

## Testing

`cargo test` runs the hardware-free suite (partition-table writing on in-memory
buffers, ISO classification, the planning logic). The full write paths can be
exercised against a loopback image without risking a real device:

```sh
sudo ./tests/loop-test.sh
```

## License

GPL-3.0-or-later.
