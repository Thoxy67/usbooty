# USBooty docs

USBooty is a Linux desktop app for creating bootable USB drives from ISO
images. It is written in Rust with a Qt 6 / QML front end, and the
bootable-media logic is ported from
[Rufus](https://github.com/pbatard/rufus).

## Contents

* [Architecture](architecture.md): the three-crate workspace and the
  privilege boundary between the GUI and the helper.
* [Write methods](write-methods.md): when to pick DD, partitioned copy,
  format only, Ventoy, or FreeDOS. Includes the hybrid MBR table option,
  read-back verify, the bad-blocks / fake-flash scan, and the SMART
  probe.
* [Windows ISOs](windows-iso.md): WIM strategy (split vs UEFI:NTFS),
  every `autounattend.xml` option, the debloat profile, the Windows CA
  2023 fix, automatic BitLocker disable, the post-install desktop
  helpers (eighteen ready-to-run `.bat` scripts on the new user's
  desktop), the rg-adguard SHA-1 lookup, and the Microsoft ISO
  downloader.
* [Linux ISOs](linux-iso.md): ISO classification, partition-based
  persistence for the Ubuntu, Debian, Fedora, openSUSE, and Arch
  families, Slax inline persistence, and the per-distro fix table
  (the archiso GRUB-redirect and the Knoppix safe-boot flags).
* [Other systems](other-systems.md): how USBooty classifies and writes
  BSD, FreeDOS, ReactOS, and other niche images.
* [Installation](installation.md): build from source, the install
  script, the AUR package, and the optional runtime tools.
* [Developing](developing.md): repo layout, running tests, the loopback
  test driver, the translation refresh, and packaging.
* [Troubleshooting](troubleshooting.md): the most common runtime
  errors, the revocation banner, fake-flash warnings, SMART warnings,
  Wayland icon notes, and recovery tips.

## Project goals

1. Be a Linux Rufus that gets the bootable-media corner cases right.
2. Never run as root for anything that does not strictly need root.
3. Stay hackable: pure Rust, no embedded shell scripts, no hidden
   state, and a tiny serializable contract between the GUI and the
   helper.
