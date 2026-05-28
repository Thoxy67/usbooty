# Windows ISOs

USBooty inspects every selected ISO and flags it as a Windows
installer when it finds `sources/install.wim` (or `install.esd`) and
a recognisable Windows boot configuration. Once detected, the
Windows-specific behaviours below kick in.

## 1. `install.wim` over the FAT32 limit

FAT32 cannot hold a file larger than 4 GiB. Modern Windows ISOs
commonly ship an `install.wim` that exceeds this, so a plain FAT32
copy would fail. When USBooty sees this case it prompts you to choose
one of two strategies.

### Split (`WimStrategy::Split` in the JSON Job)

Splits `install.wim` into 3.8 GiB `install.swm` chunks during the
copy, using `wimlib-imagex`. Windows Setup picks the chunks up
natively; no extra work at install time.

Pros: a single FAT32 partition, maximum firmware compatibility, no
bootloader patching needed.

Cons: requires `wimlib-imagex` installed locally (`wimlib` or
`wimtools` depending on your distro).

### UEFI:NTFS or UEFI:exFAT (`WimStrategy::UefiNtfs`)

Lays out two partitions: a large NTFS (or exFAT) partition that
holds the Windows files intact, and a tiny FAT32 partition at the
end of the disk that carries the Rufus UEFI:NTFS bootloader (a
signed EFI image that knows how to chainload an NTFS or exFAT
volume).

Pros: keeps `install.wim` intact, no external tool needed beyond
`mkfs.ntfs` or `mkfs.exfat`.

Cons: UEFI only, and the second partition pulls a small bootloader
image that has to be downloaded from the upstream Rufus repo on
first run.

The downloaded `uefi-ntfs.img` is cached under
`$XDG_CACHE_HOME/usbooty/` with a metadata file so the GUI can
refresh it when Rufus publishes a new one.

## 2. The Windows setup dialog (`autounattend.xml`)

When you press Start on a Windows ISO with the partitioned method, a
dialog appears with optional installer tweaks. Every field is
independent; an empty dialog produces a no-op unattend file that
Windows ignores.

The settings flow into a generated `autounattend.xml` placed on the
USB root. Windows Setup picks this file up automatically from the
install media.

### Hardware-check bypass

| Setting        | What it does                                          |
|----------------|-------------------------------------------------------|
| TPM 2.0        | Sets `BypassTPMCheck=1` in `LabConfig`                |
| Secure Boot    | Sets `BypassSecureBootCheck=1`                        |
| 8 GB RAM       | Sets `BypassRAMCheck=1`                               |
| 64 GB storage  | Sets `BypassStorageCheck=1`                           |
| Supported CPU  | Sets `BypassCPUCheck=1`                               |
| Disk geometry  | Sets `BypassDiskCheck=1`                              |

These keys are harmless on Windows 10, which silently ignores them.
On Windows 11 they let Setup proceed on hardware Microsoft considers
unsupported.

### Out-of-box experience (OOBE)

| Setting | Effect |
|---------|--------|
| Skip Microsoft-account requirement | Emits both `BypassNRO` (Win 10 and Win 11 pre-24H2) and `<HideOnlineAccountScreens>true</HideOnlineAccountScreens>` (Win 11 24H2+). |
| Disable network during OOBE | Disables every network adapter in the `specialize` pass, re-enables them in `FirstLogonCommands`. Forces local-account creation on 24H2+ even when the two flags above are ignored. |
| Skip Wi-Fi screen | `<HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>` |
| Hide OEM registration | `<HideOEMRegistrationScreen>true</HideOEMRegistrationScreen>` |
| Pre-answer network type as Work | `<NetworkLocation>Work</NetworkLocation>` (private / trusted) |
| Disable telemetry prompts | `<HideEULAPage>true</HideEULAPage>` plus `<ProtectYourPC>3</ProtectYourPC>` (the "skip Express settings" answer). |

### Local account

Filling in **Name** creates a local account during OOBE. Filling in
**Password** sets a password and also emits an `<AutoLogon>` block so
the first boot logs in directly without a prompt.

If both are empty, you get the usual OOBE account-creation flow.

### System identity

* **Computer name**: 1 to 15 characters, no whitespace, no
  `\/:*?"<>|`. The helper sanitises and truncates if you exceed
  this.
* **Locale**: a BCP-47 tag like `en-US`, `fr-FR`, `de-DE`. Applied
  to the setup UI, system locale, UI language, user locale, and the
  default keyboard layout in one go.
* **Time zone**: a Microsoft TimeZone identifier (for example `UTC`,
  `Pacific Standard Time`, `Romance Standard Time`). The picker is
  built from the canonical Microsoft catalog sorted by UTC offset.
* **Product key**: a generic VL key (the public Win 11 Pro key
  `VK7JG-NPHTM-C97JM-9MPGT-3V66T` works) lets Setup skip its
  activation prompt without actually activating the install.

There is also a **Replicate regional from host** button that copies
your Linux session's `LANG` and `TZ` (mapped to a Microsoft TimeZone
ID) into the Locale and Time-zone fields in one click.

### Setup-time extras

* **Auto-accept Setup EULA**: `<AcceptEula>true</AcceptEula>` in the
  `windowsPE` UserData block, so the Setup-time license prompt is
  skipped.
* **.NET Framework 3.5**: runs DISM in the `specialize` pass to
  enable NetFx3 from the install media's `sources\sxs` folder. No
  network required.

## 3. Privacy and debloat

The Privacy and debloat section of the dialog offers four toggles.

### Disable automatic BitLocker device encryption

Sets `HKLM\SYSTEM\CurrentControlSet\Control\BitLocker
\PreventDeviceEncryption=1` during the `specialize` pass, so Windows
11 24H2+ does not silently turn on BitLocker on first sign-in. Useful
for dual-boot setups, lab images, and any machine where another OS
might need to read the disk later. The key is valid (and harmless)
on older Windows versions that never auto-encrypt.

### Install Windows CA 2023 Secure Boot policy

Some Microsoft bootloaders are now signed by the new "Windows UEFI
CA 2023" chain. UEFI firmware that has not received the new CA via
Windows Update will refuse to boot them under Secure Boot. With
this option on, USBooty mounts the source ISO, extracts
`Windows\System32\SecureBootUpdates\SkuSiPolicy.p7b` from
`install.wim` image 1 using `wimlib-imagex`, and copies it to
`EFI\Microsoft\Boot\SkuSiPolicy.p7b` on the USB. Needs
`wimlib-imagex` on the host; the option silently no-ops on older
Windows ISOs that do not contain the policy file.

### Apply debloat profile

Writes `usbooty-debloat.reg` next to `autounattend.xml` on the USB
root and imports it during the `specialize` pass:

* Machine-wide via `HKLM` (Group Policy).
* Default-user via loading `HKU\DFT` from the default user hive,
  importing, then unloading. Every new account inherits the result.

What it disables.

**Machine-wide (HKLM, Group Policy)**

* News and Interests feed (the taskbar widget).
* Consumer-feature ads (suggested Store apps, OEM-style inserts).
* Activity History sync to Microsoft.
* Cortana in Search.
* Windows Copilot service.
* Windows Recall (the rolling-screenshot AI history; Win 11 24H2+).
* Diagnostic data downgraded to Required only.

**Default user (inherited by every new account)**

* Bing and web suggestions in Start and Search.
* File extensions shown (instead of hidden).
* Copilot, Task View, Widgets, and People buttons hidden from the
  taskbar.
* Sync-provider ads in Explorer suppressed.
* Start menu "recommendations" and Iris suggestions disabled.
* ContentDeliveryManager: lock-screen rotation ads, pre-installed
  app suggestions, "subscribed content" tiles.
* Cortana and Bing inside per-user Search.
* Advertising ID disabled.
* "Tailored experiences" derived from diagnostic data.
* "Suggested" toast notifications.
* Phone Link prompts.
* Online speech recognition (voice stays local).
* Contact harvesting for input personalization.
* Feedback Hub frequency set to Never.
* "Finish setting up your device" prompts.

Windows 11-only keys (Copilot, Widgets, News and Interests, Recall)
are silently ignored on Windows 10, so the same profile is safe on
both.

### Drop a USBooty folder on the user's Desktop (post-install helpers)

When this option is on, USBooty lays out a folder of ready-to-run
`.bat` scripts next to `autounattend.xml`. During the `specialize`
pass an `xcopy` mirrors that folder to
`C:\Users\Default\Desktop\USBooty\`, and because Windows clones
`Default\` into every new user account at OOBE, the folder ends up
on the user's actual Desktop on first sign-in. The USB can be
unplugged before first logon without breaking it.

Each script is a right-click "Run as administrator" launcher with a
`pause` at the end so the user sees the result. The bundle ships
eighteen scripts plus a README that explains each one in plain
English.

| File                                 | What it does                                                         |
|--------------------------------------|----------------------------------------------------------------------|
| `1-Win11Debloat.bat`                 | Raphire's Win11Debloat (`debloat.raphi.re`). Interactive picker.     |
| `2-ChrisTitus-Winutil.bat`           | Chris Titus Tech `winutil`, stable channel (`christitus.com/win`).   |
| `2.1-ChrisTitus-Winutil-Dev.bat`     | Same tool, dev channel (`christitus.com/windev`).                    |
| `3-Massgravel-Activator.bat`         | Microsoft Activation Scripts (`get.activated.win`).                  |
| `4-Remove-OneDrive.bat`              | Kills OneDrive and runs both x64 and WoW64 `OneDriveSetup /uninstall`. |
| `5-OfficeTool.bat`                   | Downloads the OfficeTool runtime ZIP to the user's Downloads folder. |
| `6-Install-Chocolatey.bat`           | Installs Chocolatey machine-wide. Admin required.                    |
| `7-Install-Scoop.bat`                | Installs Scoop per-user. Do not run as admin.                        |
| `8-Install-Winget.bat`               | Installs or repairs winget via `asheroto/winget-install`.            |
| `9-Remove-Windows-AI.bat`            | Strips Copilot, Recall, generative Paint / Notepad / Photos, AI Search and Cortana hooks via `zoicware/RemoveWindowsAI`. |
| `10-Winhance.bat`                    | Winhance GUI for debloat / privacy / optimisation (`get.winhance.net`). |
| `11-FR33THY-Ultimate.bat`            | FR33THY's Ultimate gaming / latency tweaks. Aggressive. Read upstream README first. |
| `12-Install-PowerToys.bat`           | Microsoft PowerToys via winget (FancyZones, PowerRename, Run, etc.). Needs winget. |
| `13-Disable-FastStartup.bat`         | Clears `HiberbootEnabled` so a dual-boot Linux can mount NTFS cleanly. Admin required. |
| `14-Enable-LongPaths.bat`            | Sets `LongPathsEnabled=1` to lift the 260-character `MAX_PATH` limit. Admin; reboot recommended. |
| `15-Install-VCRedist.bat`            | Visual C++ Redistributable 2015-2022, x64 and x86, via winget. |
| `16-Install-DirectX.bat`             | Legacy DirectX runtime (D3DX, D3DCompiler, XAudio2) via winget. |
| `17-Install-Browser.bat`             | Interactive menu: Chrome, Firefox, Brave, Zen, LibreWolf, Floorp, Waterfox, Opera, Opera GX, Vivaldi, Arc. |

Notes:

* Each script downloads code from the public internet on first run.
  Open the `.bat` in Notepad first if you want to see the exact URL
  it hits.
* All scripts ship with CRLF line endings so cmd parses them
  cleanly.
* The folder layout is sealed at build time via `include_str!`, so
  the helper does not depend on any runtime asset path.

## 4. Downloading Windows ISOs

From the **Browse** split button, **Download a Windows ISO** opens a
dialog that talks directly to Microsoft. This is a Rust port of
Rufus's Fido logic, not a wrapper around an external PowerShell
script.

Three steps:

1. Pick the Windows release (10 or 11).
2. **List languages** asks Microsoft for the languages available for
   that release; pick one.
3. **List downloads** asks Microsoft for the architectures and
   editions available for that language; **Download** then starts
   the transfer.

If Microsoft's anti-bot system rejects the request (common on VPNs,
public Wi-Fi, and some ISPs), the dialog offers a button to open the
matching Microsoft download page in your browser so you can fetch
the ISO manually.

## 5. ISO trust signals

When you load a Windows ISO, USBooty computes its SHA-1 in the
background and asks `sha1.rg-adguard.net` whether the hash matches
a published Microsoft retail, volume, or OEM build. The result shows
up next to the SHA-1 in the digest panel as a small badge:
**Retail**, **Volume**, **OEM**, or **Unknown**. An Unknown badge
just means the upstream catalog does not list that exact hash; it
is not by itself a sign that the ISO has been tampered with.

If the SBAT or DBX scan flags any of the ISO's signed EFI binaries
as revoked by current Secure Boot policy, a red banner appears
under the ISO summary explaining which binary is flagged and what
to try instead (a newer ISO, or booting in legacy / non-Secure-Boot
mode). See [Troubleshooting](troubleshooting.md#revocation-banner).
