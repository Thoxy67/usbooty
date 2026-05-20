# usbooty docs

usbooty is a Linux desktop app for creating bootable USB drives from ISO
images. It is written in Rust with a Qt 6 / QML front end, and the
bootable-media logic is ported from [Rufus](https://github.com/pbatard/rufus).

## Contents

* [Architecture](architecture.md): the three-crate workspace and the privilege
  boundary between the GUI and the helper.
* [Write methods](write-methods.md): when to pick DD, partitioned copy, plain
  format, or Ventoy.
* [Windows ISOs](windows-iso.md): UEFI:NTFS, `install.wim` strategy, the
  `autounattend.xml` options, the debloat profile, and the Fido downloader.
* [Linux ISOs](linux-iso.md): ISO classification and persistent overlays for
  Debian / Ubuntu live systems.
* [Installation](installation.md): build from source, the install script, and
  the AUR package.
* [Developing](developing.md): repo layout, running tests, the loopback test
  driver, and packaging.
* [Troubleshooting](troubleshooting.md): common runtime errors, Wayland icon
  notes, and recovery tips.

## Project goals

1. Be a Linux Rufus that gets the bootable-media corner cases right.
2. Never run as root for anything that does not strictly need root.
3. Stay hackable: pure Rust, no embedded shell scripts, no hidden state, and
   a tiny serializable contract between the GUI and the helper.
