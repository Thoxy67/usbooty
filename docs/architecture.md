# Architecture

USBooty is a Cargo workspace of three crates, separated along a
privilege boundary.

```text
+----------------------+        pkexec        +-----------------------+
| usbooty-gui (Qt/QML) | -------------------> | usbooty-helper (CLI)  |
|  unprivileged user   |  Job JSON on stdin   |        root           |
|                      | <------------------- |                       |
|                      |  ProgressMsg on out  |                       |
+----------------------+                      +-----------------------+
              \                                          /
               \---- usbooty-core (shared types) -------/
```

## The three crates

### `usbooty-core`

A small, dependency-light crate (only `serde`) containing:

* The serializable types passed over the privilege boundary: `Job`,
  `JobOptions`, `WindowsSetup`, `Persistence`, `ProgressMsg`,
  `IsoReport`, `DeviceInfo`, `RevocationDb`.
* Pure decision logic that classifies an ISO and plans a layout
  (`plan::auto_filesystem`, `iso_report::IsoReport`,
  `uefi_ntfs::validate_uefi_ntfs`, `revocation::scan_efi_binaries`).

Nothing here touches Qt, the network, real hardware, or root. That
means it can be exhaustively unit-tested in CI without sudo or attached
disks.

### `usbooty-gui`

The unprivileged Qt 6 / QML application (binary `usbooty`).
Responsibilities:

* Enumerate removable devices via sysfs.
* Analyse the source ISO (mount via FUSE if available, fall back to
  the embedded ISO9660 reader otherwise).
* Transparent decompression of `.xz`, `.gz`, `.bz2`, `.zst`, `.lzma`,
  `.zip`, `.Z`, and fixed `.vhd` inputs via `decompress.rs`.
* Compute every digest (MD5, SHA-1, SHA-256, SHA-512, BLAKE3) of the
  source for display, in one read pass.
* Cross-check the SHA-1 against `sha1.rg-adguard.net` and label the
  result (Retail, Volume, OEM, unknown).
* Scan the ISO for SBAT generations and DBX-revoked Authenticode
  hashes, surface the result as a red banner.
* Probe the selected block device with `smartctl` for reallocated
  sectors / temperature / failing prediction.
* Download the Rufus `uefi-ntfs.img` runtime resource and the latest
  FreeDOS kernel and shell into the user cache.
* Download Windows 10 / 11 ISOs from Microsoft (the ported Fido
  logic).
* Build a `Job`, prompt for confirmation, and launch the helper.
* Live-switch the GUI language between French and forced English.

### `usbooty-helper`

A small privileged CLI binary with no GUI and no network access. It
reads a `Job` as JSON on stdin, executes it, and streams `ProgressMsg`
JSON lines on stdout. Writing the string `cancel\n` to its stdin (or
closing stdin) aborts the job cleanly.

The helper is the only component that opens block devices, runs
`mkfs.*` tools, or writes partition tables. Every device mutation
happens through it.

Key helper modules:

* `dd.rs`, `partitioned.rs`, `format.rs`, `ventoy.rs`, `freedos.rs`:
  the five write methods.
* `partition.rs`: GPT, MBR, hybrid MBR / GPT, and BIOS+UEFI CSM table
  writers.
* `uefi_ntfs.rs`: the small FAT tail partition that carries the Rufus
  EFI bootloader (works for NTFS and exFAT main partitions).
* `winca2023.rs`: copies `SkuSiPolicy.p7b` from `install.wim` so older
  UEFI firmware can boot the Windows CA 2023 chain.
* `unattend.rs`: generates `autounattend.xml`, the debloat policy
  import, the BitLocker auto-encryption guard, and the post-install
  desktop helpers (eleven ready-to-run `.bat` scripts xcopied into
  `C:\Users\Default\Desktop\USBooty\`).
* `wimsplit.rs`: chunks an oversized `install.wim` into `install.swm`
  parts on a FAT32 target.
* `persistence.rs`, `distro_fixes.rs`: per-distro overlays and quirks
  (Debian `persistence.conf`, casper `persistent` parameter, Slax
  inline `slax/changes/`, Manjaro `efi_boot_img` paths, etc.).
* `backup.rs`: drive-to-image snapshot, the inverse of writing.
* `check.rs`: Quick (F3-style fake-capacity / fake-flash) and Full
  (two-pattern bad-blocks) device check modes.
* `devlock.rs`: refuses to touch a device that is currently held by
  another writer.
* `vhd.rs`: fixed `.vhd` input and `.vhd` backup output (footer-aware).

## The privilege boundary

* The GUI never has root. It plans, downloads, computes hashes, and
  displays.
* The helper is invoked once per write, via `pkexec`. There is a
  single polkit prompt per job.
* The contract is the JSON `Job` schema in `usbooty-core`. The helper
  makes no policy decisions: if the GUI sends `WimStrategy::Split`,
  the helper splits. It does not second-guess.

This split has two practical wins:

1. Bugs in ISO analysis, network code, or QML cannot escalate to a
   wipe of the wrong device, because the GUI cannot touch a block
   device at all.
2. The helper is small enough to read end-to-end before trusting it
   with root.

## Message flow

```
GUI                                       Helper
 |                                          |
 |  spawn `pkexec usbooty-helper`           |
 |----------------------------------------->|
 |                                          |
 |  write Job JSON, then "\n"               |
 |----------------------------------------->|
 |                                          |
 |     {"type":"phase","name":"Writing"}    |
 |<-----------------------------------------|
 |     {"type":"progress","done":...,...}   |
 |<-----------------------------------------|
 |     {"type":"log","level":"info",...}    |
 |<-----------------------------------------|
 |     {"type":"done","ok":true}            |
 |<-----------------------------------------|
 |  exit 0                                  |
 |                                          |
```

If the user clicks Cancel, the GUI writes `cancel\n` on the helper's
stdin. The helper checks its `AtomicBool` between every chunk and
bails cleanly, leaving the device in a partial but predictable state.

## Repo layout

```
crates/
  core/        usbooty-core: shared types and pure logic
  gui/         usbooty-gui: the Qt 6 / QML binary
    qml/       main.qml
    qrc/       Qt resource bundles (icon, translations)
    src/       Rust source (bridge, runner, decompress, etc.)
  helper/      usbooty-helper: the privileged CLI binary
    src/
data/          desktop file, AppStream metadata, icons, polkit policy,
               French translations
packaging/     PKGBUILD and AUR helper script
tests/         loop-test.sh (hardware-free end-to-end driver)
docs/          you are here
```

User preferences (Force English, Always show activity log) live in
`~/.config/usbooty/settings.json` via `directories::ProjectDirs`.
Cached downloads (the UEFI:NTFS image, FreeDOS kernel and shell, the
DBX revocation file) live under `$XDG_CACHE_HOME/usbooty/` and are
refreshed on demand.

## Why this shape

Most Linux ISO writers either run the entire app as root (bad: the
whole network and rendering stack inherits root) or shell out to a
soup of `mkfs.*`, `parted`, `dd`, and `cp` (bad: hard to reason about,
no real contract). USBooty picks a third path: a thin privileged
worker with a narrow, typed input, and a fat unprivileged front end
that does everything else.

The result is that the most dangerous code path (device mutation) is
the shortest one, and it is the easiest to audit.
