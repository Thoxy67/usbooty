# Architecture

usbooty is a Cargo workspace of three crates, separated along a privilege
boundary.

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

A tiny, dependency-light crate (only `serde`) containing:

* The serializable types passed over the privilege boundary: `Job`,
  `JobOptions`, `WindowsSetup`, `Persistence`, `ProgressMsg`, `IsoReport`,
  `DeviceInfo`.
* Pure decision logic that classifies an ISO and plans a layout
  (`plan::auto_filesystem`, `iso_report::IsoReport`,
  `uefi_ntfs::validate_uefi_ntfs`).

Nothing here touches Qt, the network, real hardware, or root. That means it
can be exhaustively unit-tested in CI without sudo or attached disks.

### `usbooty-gui`

The unprivileged Qt 6 / QML application (binary `usbooty`). Responsibilities:

* Enumerate removable devices via sysfs.
* Analyse the source ISO (mount via FUSE if available, fall back to the
  embedded ISO9660 reader otherwise).
* Compute the SHA-256 of the source for display.
* Download the Rufus `uefi-ntfs.img` runtime resource into the user cache.
* Download Windows 11 ISOs from Microsoft (the ported Fido logic).
* Build a `Job`, prompt for confirmation, and launch the helper.

### `usbooty-helper`

A small privileged CLI binary with no GUI and no network access. It reads a
`Job` as JSON on stdin, executes it, and streams `ProgressMsg` JSON lines on
stdout. Writing the string `cancel\n` to its stdin (or simply closing stdin)
aborts the job cleanly.

The helper is the only component that opens block devices, runs `mkfs.*`
tools, or writes partition tables. Every device mutation happens through it.

## The privilege boundary

* The GUI never has root. It plans, downloads, computes hashes, and displays.
* The helper is invoked once per write, via `pkexec`. There is a single
  polkit prompt per job.
* The contract is the JSON `Job` schema in `usbooty-core`. The helper makes
  no policy decisions: if the GUI sends `WimStrategy::Split`, the helper
  splits; it does not second-guess.

This split has two practical wins:

1. Bugs in ISO analysis, network code, or QML cannot escalate to a wipe of
   the wrong device, because the GUI cannot touch a block device at all.
2. The helper is small enough to read end-to-end before trusting it with root.

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

If the user clicks Cancel, the GUI writes `cancel\n` on the helper's stdin.
The helper checks its `AtomicBool` between every chunk and bails cleanly,
leaving the device in a partial but predictable state.

## Repo layout

```
crates/
  core/        usbooty-core: shared types and pure logic
  gui/         usbooty-gui: the Qt 6 / QML binary
    qml/       main.qml
    qrc/       Qt resource bundles (the embedded icon)
    src/       Rust source
  helper/      usbooty-helper: the privileged CLI binary
    src/
data/          desktop file, AppStream metadata, icons, polkit policy
packaging/     PKGBUILD and AUR helper script
tests/         loop-test.sh (hardware-free end-to-end driver)
docs/          you are here
```

## Why this shape

Most Linux ISO writers either run the entire app as root (bad: the whole
network and rendering stack inherits root) or shell out to a soup of
`mkfs.*`, `parted`, `dd`, and `cp` (bad: hard to reason about, no real
contract). usbooty picks a third path: a thin privileged worker with a
narrow, typed input, and a fat unprivileged front end that does everything
else.

The result is that the most dangerous code path (device mutation) is the
shortest one, and it is the easiest to audit.
