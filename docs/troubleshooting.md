# Troubleshooting

## "Some required tools are missing" banner at the top of the window

USBooty checks for `pkexec` and several `mkfs.*` tools at startup.
The banner lists what is missing. Install them as listed in
[installation.md](installation.md#runtime-dependencies).

The filesystem combo in the partition section only lists filesystems
whose `mkfs.*` tool is actually present on the host, so the choice
is never a silent failure.

## pkexec prompts twice, or fails silently

Check that the polkit policy is installed and that polkit picked it
up:

```sh
ls /usr/share/polkit-1/actions/org.usbooty.helper.policy
pkaction --action-id org.usbooty.helper.run
```

If the second command returns nothing, the policy is not registered.
Restart the polkit daemon (`systemctl restart polkit`) or log out
and back in.

## The QEMU boot test does not appear in the Device menu

The **Verify boot device (QEMU)** entry is shown only when
`qemu-system-x86_64` is installed and `/dev/kvm` exists (KVM
acceleration available). Install `qemu-full` (Arch) or
`qemu-system-x86` (Debian/Ubuntu). Inside the dialog:

* The UEFI firmware options need OVMF (`edk2-ovmf` / `ovmf`); the
  Secure Boot variant additionally needs the `.secboot` OVMF build.
* The virtual TPM 2.0 (needed by Windows 11 OOBE) needs `swtpm`.

Launching a boot test asks for your password once (pkexec): reading
a raw block device needs root. In snapshot mode (the default) the
device itself is never modified; all guest writes go to a throwaway
overlay.

If QEMU exits immediately, the reason lands in the activity log. The
two common ones: the device is held by another process (close any
other QEMU window using it), and a guest-audio backend failure (try
unchecking Guest audio).

## The Windows titlebar icon is the generic Wayland one

Wayland compositors look up the window icon from an installed
`.desktop` file, matching the app's `xdg-toplevel app_id` against
the desktop file's basename. For installed builds (AUR or
`install.sh`), the desktop file is at
`/usr/share/applications/org.usbooty.Usbooty.desktop` and the icon
shows up in the titlebar normally.

For dev builds (`cargo run`), the desktop file is not installed by
default. Install it into your user data dir once (see
[developing.md](developing.md#build-for-development)).

USBooty sets the app_id via `QGuiApplication::setDesktopFileName` in
`crates/gui/src/main.rs`, so the match works in both KDE and GNOME
without any extra config.

## "Verify failed" after a write

Verify reads the written data back and compares its hash to a hash
captured during the write. If verify fails, the write itself was
not bit-for-bit correct. Common causes:

* A failing USB stick. Try a different drive.
* A USB hub or front-panel header that drops bytes. Plug the drive
  directly into a motherboard port.
* For DD writes specifically: an ISO whose own contents are
  corrupted (a truncated download). Re-fetch and compare the
  SHA-256 (or BLAKE3) published by the distro.

USBooty computes all five common digests in one read pass; both
MD5 / SHA-1 / SHA-256 / SHA-512 / BLAKE3 show up in the digests
panel so you can match whichever the upstream publisher used.

## "This device looks like a fake-capacity USB stick" warning

The Quick check writes a sparse pattern at strategic offsets and
reads it back. A failure means the device is reporting more capacity
than the flash actually has (the classic "16 GB stick that is really
2 GB"). Stop using the drive: any data written past the real
capacity is silently dropped.

If you want a more thorough scan, run the Full check, which writes
a `0x55` / `0xAA` pattern across the whole device and catches stuck
blocks. Both modes are destructive: they will wipe the drive.

## SMART warning chip under the device picker

A background SMART probe runs against the selected device using
`smartctl`. If it finds reallocated sectors, high temperatures, or
a failing-prediction flag, a short yellow chip appears under the
picker. The drive may still work for the immediate write but is
showing signs of wear; consider replacing it before relying on it
for backups.

If `smartmontools` is not installed, there is no probe and no chip.

## Revocation banner

When you load a Windows or Linux ISO, USBooty scans every signed
EFI binary inside it against:

* The baked-in SBAT generation table (current Microsoft published
  generations), AND
* The live UEFI Forum DBX revocation list (downloaded and cached
  under `$XDG_CACHE_HOME/usbooty/`).

If any binary is flagged as revoked or obsolete, a red banner
appears under the ISO summary explaining which one. UEFI firmware
with current revocation data will refuse to load it. Workarounds:

* Try a newer ISO (most distros refresh shim and grub when this
  happens).
* Boot in legacy / non-Secure-Boot mode.
* For Windows installers specifically: enable **Install Windows CA
  2023 Secure Boot policy** in the Windows-setup dialog, which
  copies `SkuSiPolicy.p7b` to the USB so firmware that has the new
  CA accepts the chain. This only helps on the very latest Windows
  ISOs and only on firmware that has not yet picked up the new CA
  via Windows Update.

The DBX file is refreshed on demand; you can delete it from the
cache to force a re-download.

## "could not compile usbooty-gui" with undefined cxxbridge or qguiapplication symbols

This is a rust-lld + GCC LTO bitcode mismatch. Either:

* You are building with `RUSTFLAGS` or a project profile that asks
  for LTO while the C++ side compiles without it, or
* You are running `makepkg` on a PKGBUILD that does not have
  `options=('!lto')`.

Fix: add `options=('!lto')` to the PKGBUILD, or unset the
conflicting `RUSTFLAGS`. The long-form explanation is in
`packaging/PKGBUILD`.

## "Microsoft rejected the download request"

Microsoft has anti-bot rules on the consumer ISO download endpoint,
and they sometimes flag VPN exit IPs, public Wi-Fi networks, or
specific user-agent patterns. When this happens:

1. The download dialog shows the error in its status label.
2. Click **Open Microsoft download page** to open the matching
   consumer download page in your browser, and download manually
   from there.
3. Use **Browse** to load the resulting ISO into USBooty as usual.

## "device too small" error

USBooty refuses to write a layout that would not fit on the target.
Most often this happens with the UEFI:NTFS or UEFI:exFAT strategies
on small drives: the main partition needs to hold the entire
Windows ISO contents, plus a tiny FAT partition at the tail for the
bootloader. Use a larger USB stick.

For the FreeDOS method, the FAT16 variant caps at 4 GB partitions
on most BIOSes; if the drive is bigger and FAT16 fails, pick FAT32.

## "Another usbooty job is already running" (devlock)

To stop two usbooty writes racing on the same device, the helper
takes an exclusive `flock` on `/run/usbooty-<device>.lock` as its
first action. A second usbooty job aimed at the same device aborts
immediately with `another usbooty job is already running on
<device>`, before any destructive step. Wait for the first job to
finish; the lock clears automatically when that helper exits, even
on an unclean exit.

A device held open by *another* program (a file manager preview, a
stale `dd`, or a still-mounted partition) is a separate case. The GUI
unmounts the target's partitions with `udisksctl` before starting,
and the helper opens the device with `O_EXCL`, so a still-busy device
is refused rather than half-written. Close the program holding it (or
unmount the partition) and try again.

## A device shows up that I do not want to touch

Make sure **Show non-removable (internal) disks** is unchecked (it
is off by default). With it off, USBooty only enumerates removable
USB devices.

If a removable drive looks wrong (vendor or model do not match),
click the refresh icon next to the device picker and cross-check
against `lsblk -d -o NAME,VENDOR,MODEL,SIZE,TRAN`. The confirmation
dialog before a write spells out exactly which device will be
erased.

## I can't see the activity log

By default the log panel auto-opens when the first log line
arrives. If you want it always visible (for example to watch a
slow write progress in real time), toggle **Always show activity
log** under the `?` menu. The setting persists in
`~/.config/usbooty/settings.json`.

You can also save the current log to a file from the disk icon in
the log header (Save log to file).

## The GUI came up in French and I want English (or vice versa)

USBooty follows your `LANG` / `LC_ALL` locale for the GUI language.
French is the only non-English translation currently shipped.

Toggle **Force English** under the `?` menu to opt out of locale
translation and run the GUI in its source language. The change
applies live, no restart needed. The preference is saved in
`~/.config/usbooty/settings.json`.

## A post-install Windows desktop script does nothing or errors out

The twenty-six `.bat` scripts dropped by **Drop a USBooty folder on
the user's Desktop** are thin wrappers around well-known upstream
PowerShell snippets. They all download code from the public
internet on first run, so:

* No network? Connect first.
* Corporate proxy? The underlying tool may not respect it; check
  the upstream documentation for the relevant script.
* Wrong run mode? Most scripts need admin (right-click "Run as
  administrator"); Scoop is per-user and must NOT be run as admin.

If a script returns an error, open the `.bat` in Notepad to see
the exact upstream URL it hits, and consult that project's issue
tracker. The bundled README.txt next to the scripts lists each
upstream homepage.
