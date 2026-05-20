# Troubleshooting

## "Some required tools are missing" banner at the top of the window

usbooty checks for `pkexec` and several `mkfs.*` tools at startup. The banner
lists what is missing. Install them as listed in
[installation.md](installation.md#runtime-dependencies).

## pkexec prompts twice, or fails silently

Check that the polkit policy is installed and that polkit picked it up:

```sh
ls /usr/share/polkit-1/actions/org.usbooty.helper.policy
pkaction --action-id org.usbooty.helper.run
```

If the second command returns nothing, the policy is not registered. Restart
the polkit daemon (`systemctl restart polkit`) or log out and back in.

## The Windows titlebar icon is the generic Wayland one

Wayland compositors look up the window icon from an installed `.desktop`
file, matching the app's `xdg-toplevel app_id` against the desktop file's
basename. For installed builds (AUR or `install.sh`), the desktop file is at
`/usr/share/applications/org.usbooty.Usbooty.desktop` and the icon shows up
in the titlebar normally.

For dev builds (`cargo run`), the desktop file is not installed by default.
Install it into your user data dir once (see
[developing.md](developing.md#build-for-development)).

usbooty sets the app_id via `QGuiApplication::setDesktopFileName` in
`crates/gui/src/main.rs`, so the match works in both KDE and GNOME without
any extra config.

## "Verify failed" after a write

Verify reads the written data back and compares its hash to a hash captured
during the write. If verify fails, the write itself was not bit-for-bit
correct. Common causes:

* A failing USB stick. Try a different drive.
* A USB hub or front-panel header that drops bytes. Plug the drive directly
  into a motherboard port.
* For DD writes specifically: an ISO whose own contents are corrupted (a
  truncated download). Re-fetch and check the SHA-256 published by the
  distro.

## "could not compile usbooty-gui" with undefined cxxbridge or qguiapplication symbols

This is a rust-lld + GCC LTO bitcode mismatch. Either:

* You are building with `RUSTFLAGS` or a project profile that asks for LTO
  while the C++ side compiles without it, or
* You are running `makepkg` on a PKGBUILD that does not have
  `options=('!lto')`.

Fix: add `options=('!lto')` to the PKGBUILD, or unset the conflicting
`RUSTFLAGS`. The long-form explanation is in `packaging/PKGBUILD`.

## "Microsoft rejected the download request"

Microsoft has anti-bot rules on the consumer ISO download endpoint, and
they sometimes flag VPN exit IPs, public Wi-Fi networks, or specific
user-agent patterns. When this happens:

1. The download dialog shows the error in its status label.
2. Click **Open Microsoft download page** to open the matching consumer
   download page in your browser, and download manually from there.
3. Use **Browse** to load the resulting ISO into usbooty as usual.

## ISO mount fails: "fusermount3 not found"

usbooty mounts the source ISO via FUSE to read its contents (file list,
SHA-256, classification). It needs `fuse3` installed and either the `fuse`
group or polkit-configured fusermount.

On Arch:

```sh
sudo pacman -S --needed fuse3
```

If you cannot install FUSE, usbooty falls back to its embedded ISO9660
reader. That works for read-only metadata but cannot extract files for the
partitioned copy method. The DD method does not need FUSE either way.

## "device too small" error

usbooty refuses to write a layout that would not fit on the target. Most
often this happens with the UEFI:NTFS strategy on small drives: the NTFS
partition needs to hold the entire Windows ISO contents, plus a tiny FAT
partition at the tail for the bootloader. Use a larger USB stick.

## A device shows up that I do not want to touch

Make sure **Show non-removable (internal) disks** is unchecked (it is off
by default). With it off, usbooty only enumerates removable USB devices.

If a removable drive looks wrong (vendor or model do not match), refresh
the list and cross-check against `lsblk -d -o NAME,VENDOR,MODEL,SIZE,TRAN`.
The confirmation dialog before a write spells out exactly which device will
be erased.
