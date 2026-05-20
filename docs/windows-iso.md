# Windows ISOs

usbooty inspects every selected ISO and flags it as a Windows installer when
it finds `sources/install.wim` (or `install.esd`) and a recognisable Windows
boot configuration. Once detected, three Windows-specific behaviours kick in.

## 1. `install.wim` over the FAT32 limit

FAT32 cannot hold a file larger than 4 GiB. Modern Windows ISOs commonly
ship an `install.wim` that exceeds this, so a plain FAT32 copy would fail.
When usbooty sees this case, it prompts you to choose one of two strategies.

### Split (`WimStrategy::Split` in the JSON Job)

Splits `install.wim` into 3.8 GiB `install.swm` chunks during the copy, using
`wimlib-imagex`. Windows Setup picks the chunks up natively; no extra work
at install time.

Pros: a single FAT32 partition, maximum firmware compatibility, no
bootloader patching needed.

Cons: requires `wimlib-imagex` installed locally (`wimlib` / `wimtools`).

### UEFI:NTFS (`WimStrategy::UefiNtfs`)

Lays out two partitions: a large NTFS partition that holds the Windows files
intact, and a tiny FAT32 partition at the end of the disk that carries the
Rufus UEFI:NTFS bootloader (a signed EFI image that knows how to chainload
an NTFS volume).

Pros: keeps `install.wim` intact, no external tool needed.

Cons: UEFI only, and the second partition pulls a small bootloader image
that has to be downloaded from the upstream Rufus repo on first run.

The downloaded `uefi-ntfs.img` is cached under `$XDG_CACHE_HOME/usbooty/`
with a metadata file so the GUI can refresh it when Rufus publishes a new
one.

## 2. The Windows setup dialog (`autounattend.xml`)

When you press Start on a Windows ISO with the partitioned method, a dialog
appears with optional installer tweaks. Every field is independent; an empty
dialog produces a no-op unattend file that Windows ignores.

The settings flow into a generated `autounattend.xml` placed on the USB
root. Windows Setup picks this file up automatically from the install media.

### Hardware-check bypass

| Setting        | What it does                                          |
|----------------|-------------------------------------------------------|
| TPM 2.0        | Sets `BypassTPMCheck=1` in `LabConfig`                |
| Secure Boot    | Sets `BypassSecureBootCheck=1`                        |
| 8 GB RAM       | Sets `BypassRAMCheck=1`                               |
| 64 GB storage  | Sets `BypassStorageCheck=1`                           |
| Supported CPU  | Sets `BypassCPUCheck=1`                               |
| Disk geometry  | Sets `BypassDiskCheck=1`                              |

These keys are harmless on Windows 10, which silently ignores them. On
Windows 11 they let Setup proceed on hardware Microsoft considers
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
**Password** sets a password and also emits an `<AutoLogon>` block so the
first boot logs in directly without a prompt.

If both are empty, you get the usual OOBE account-creation flow.

### System identity

* **Computer name**: 1 to 15 characters, no whitespace, no `\/:*?"<>|`. The
  helper sanitises and truncates if you exceed this.
* **Locale**: a BCP-47 tag like `en-US`, `fr-FR`, `de-DE`. Applied to the
  setup UI, system locale, UI language, user locale, and the default
  keyboard layout in one go.
* **Time zone**: a Microsoft TimeZone identifier (for example `UTC`,
  `Pacific Standard Time`, `Romance Standard Time`). The picker is built
  from the canonical Microsoft catalog sorted by UTC offset.
* **Product key**: a generic VL key (the public Win 11 Pro key
  `VK7JG-NPHTM-C97JM-9MPGT-3V66T` works) lets Setup skip its activation
  prompt without actually activating the install.

### Setup-time extras

* **Auto-accept Setup EULA**: `<AcceptEula>true</AcceptEula>` in the
  `windowsPE` UserData block, so the Setup-time license prompt is skipped.
* **.NET Framework 3.5**: runs DISM in the `specialize` pass to enable
  NetFx3 from the install media's `sources\sxs` folder. No network required.

## 3. The debloat profile

When **Apply debloat profile** is on, usbooty writes `usbooty-debloat.reg`
to the USB root and imports it during the `specialize` pass:

* Machine-wide via `HKLM` (Group Policy).
* Default-user via loading `HKU\DFT` from the default user hive, importing,
  then unloading. Every new account inherits the result.

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
* Copilot, Task View, Widgets, and People buttons hidden from the taskbar.
* Sync-provider ads in Explorer suppressed.
* Start menu "recommendations" and Iris suggestions disabled.
* ContentDeliveryManager: lock-screen rotation ads, pre-installed app
  suggestions, "subscribed content" tiles.
* Cortana and Bing inside per-user Search.
* Advertising ID disabled.
* "Tailored experiences" derived from diagnostic data.
* "Suggested" toast notifications.
* Phone Link prompts.
* Online speech recognition (voice stays local).
* Contact harvesting for input personalization.
* Feedback Hub frequency set to Never.
* "Finish setting up your device" prompts.

Windows 11-only keys (Copilot, Widgets, News and Interests, Recall) are
silently ignored on Windows 10, so the same profile is safe on both.

## 4. Downloading Windows ISOs

From the **Browse** split button, **Download a Windows ISO** opens a dialog
that talks directly to Microsoft. This is a Rust port of Rufus's Fido logic,
not a wrapper around an external PowerShell script.

Three steps:

1. Pick the Windows release (10 or 11).
2. **List languages** asks Microsoft for the languages available for that
   release; pick one.
3. **List downloads** asks Microsoft for the architectures and editions
   available for that language; **Download** then starts the transfer.

If Microsoft's anti-bot system rejects the request (common on VPNs, public
Wi-Fi, and some ISPs), the dialog offers a button to open the matching
Microsoft download page in your browser so you can fetch the ISO manually.
