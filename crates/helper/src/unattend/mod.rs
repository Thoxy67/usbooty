//! Generating a Windows `autounattend.xml` to customize the installation.
//!
//! The file is written to the root of the USB; Windows Setup reads it
//! automatically. The Windows 11 hardware-check bypasses go in the `windowsPE`
//! pass as `LabConfig` registry writes, the same approach Rufus uses, needing
//! no offline editing of the boot image.
//!
//! Every setting here is cross-version safe: each element has been in the
//! unattend schema since Windows 10 1809, or is silently ignored on Windows 10
//! (e.g. the Windows 11 hardware-bypass registry keys sit unused on Setup's
//! HKLM, and Windows 11-only Group Policy keys in `debloat.reg` are no-ops on
//! Windows 10). A single [`WindowsSetup`] therefore works across Windows 10,
//! Windows 11 pre-24H2, Windows 11 24H2, and Windows 11 25H2+.
//!
//! Components are emitted once per supported processor architecture so the
//! same unattend file works on amd64, arm64, and x86 Windows installs.
//!
//! ## Module layout
//!
//! The three setup passes map to three sibling modules so each "what gets
//! emitted at this stage" question has one file:
//!
//! * [`windows_pe`]: installer-time settings (Win 11 bypasses, product key,
//!   Setup-UI locale).
//! * [`specialize`]: post-image, pre-OOBE settings (BypassNRO, .NET 3.5,
//!   computer name, time zone, debloat / desktop-helpers imports).
//! * [`oobe`]: first-boot user-facing settings (OOBE hide flags, auto-logon,
//!   FirstLogonCommands, local-account creation, user-facing locale).
//!
//! [`assets`] holds the embedded data those passes reference (the debloat
//! `.reg`, the desktop-helpers `.bat` bundle, the architecture list, and the
//! PowerShell one-liners). The remaining helpers in this file are XML-shape
//! primitives shared between all three pass modules.

mod assets;
mod oobe;
mod specialize;
mod windows_pe;

use anyhow::{Context, Result};
use std::path::Path;

use usbooty_core::WindowsSetup;

use crate::emit;

use assets::{
    ARCHITECTURES, DEBLOAT_REG, DEBLOAT_REG_NAME, DESKTOP_HELPERS, DESKTOP_HELPERS_DIR, EI_CFG,
    EI_CFG_DIR, EI_CFG_NAME,
};

/// Write the autounattend (and, if requested, the debloat policy) into the
/// root of the mounted target.
pub fn write(mount: &Path, setup: &WindowsSetup) -> Result<()> {
    let xml = generate(setup);
    let xml_path = mount.join("autounattend.xml");
    std::fs::write(&xml_path, &xml).with_context(|| format!("writing {}", xml_path.display()))?;

    if setup.apply_debloat {
        let reg_path = mount.join(DEBLOAT_REG_NAME);
        std::fs::write(&reg_path, DEBLOAT_REG)
            .with_context(|| format!("writing {}", reg_path.display()))?;
        emit::log("Wrote usbooty-debloat.reg alongside autounattend.xml");
    }

    if setup.desktop_helpers {
        // Lay out `<mount>/USBooty/{bat,readme}` at the install-media root; the
        // specialize pass xcopies it onto the Default user's Desktop at setup.
        write_helpers_into(&mount.join(DESKTOP_HELPERS_DIR))?;
        emit::log("Wrote USBooty/ post-install helpers next to autounattend.xml");
    }

    if setup.force_edition_picker {
        // `sources/` already exists on a Windows install USB (it's where
        // `install.wim` lives). On the off-chance the partition layout is
        // unusual or the directory is missing, `create_dir_all` is a no-op
        // when it already exists.
        let dir = mount.join(EI_CFG_DIR);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(EI_CFG_NAME);
        std::fs::write(&path, EI_CFG).with_context(|| format!("writing {}", path.display()))?;
        emit::log("Wrote sources/ei.cfg to force Setup's edition picker on boot");
    }

    emit::log("Applied Windows customization (autounattend.xml)");
    Ok(())
}

/// Build the `autounattend.xml` document for the requested customizations.
pub fn generate(setup: &WindowsSetup) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str(
        "<unattend xmlns=\"urn:schemas-microsoft-com:unattend\" \
         xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n",
    );
    windows_pe::push_windows_pe(&mut s, setup);
    specialize::push_specialize(&mut s, setup);
    oobe::push_oobe_system(&mut s, setup);
    s.push_str("</unattend>\n");
    s
}

/// Build an offline `unattend.xml` for an already-applied image (Windows To
/// Go): the `specialize` and `oobeSystem` passes only. There is no `windowsPE`
/// pass; Windows Setup never runs in WTG, so installer-time settings (the
/// Win 11 hardware bypasses, the Setup product key, EULA auto-accept, the
/// edition-picker `ei.cfg`) have nothing to act on and are dropped. Likewise
/// the features that stage assets onto the *install media* (.NET 3.5 from
/// `sources\sxs`, the debloat `.reg`, the desktop-helpers folder) are dropped,
/// since in WTG the USB is the running system, not a separate installer.
///
/// First boot runs the **normal Windows OOBE** (the account-creation flow),
/// matching Rufus's default Windows To Go behavior. `specialize` still applies
/// harmless machine settings (computer name, time zone, BypassNRO), and
/// `oobeSystem` ([`oobe::push_oobe_system_wtg`]) carries the user's chosen OOBE
/// hide flags / locale and, if a local account was requested, a Rufus-style
/// empty-password account (no auto-logon, password change forced at first
/// logon). Because there is no auto-logon the interactive first-sign-in still
/// runs and satisfies the modern OOBE controller. The 24H2/25H2 reseal loop
/// ("Why did my PC restart?") is prevented at the BCD/registry layer instead
/// (recovery store + `recoveryenabled=No` + `SanPolicy=4`), the same way Rufus
/// does, rather than by an Audit-mode redirect.
pub fn generate_offline(setup: &WindowsSetup) -> String {
    let setup = offline_setup(setup);
    let mut s = String::with_capacity(4096);
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str(
        "<unattend xmlns=\"urn:schemas-microsoft-com:unattend\" \
         xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n",
    );
    specialize::push_specialize(&mut s, &setup);
    oobe::push_oobe_system_wtg(&mut s, &setup);
    s.push_str("</unattend>\n");
    s
}

/// Write the offline unattend to `<windows_root>\Panther\unattend.xml`, which
/// Windows reads automatically on the first boot of an applied image. A WTG
/// image always gets one so the `specialize` machine settings (BypassNRO,
/// computer name, time zone) apply; first boot then runs the normal OOBE. When
/// the user picked no customizations the file is `specialize`-only (or nearly
/// empty), which is harmless and still lets the native OOBE run, matching Rufus.
pub fn write_offline(windows_root: &Path, setup: &WindowsSetup) -> Result<()> {
    let panther = windows_root.join("Panther");
    std::fs::create_dir_all(&panther)
        .with_context(|| format!("creating {}", panther.display()))?;
    let xml_path = panther.join("unattend.xml");
    std::fs::write(&xml_path, generate_offline(setup))
        .with_context(|| format!("writing {}", xml_path.display()))?;
    emit::log("Wrote Windows\\Panther\\unattend.xml (first boot runs the normal OOBE)");
    Ok(())
}

/// Apply the debloat policy directly to an *already-applied* Windows To Go image
/// at `mount_root` (the NTFS partition root). The install path imports
/// `usbooty-debloat.reg` from the USB media during the `specialize` pass, which
/// a running WTG system has no way to reach; here we instead merge the same
/// policy straight into the image's offline hives: the `HKLM\SOFTWARE` keys into
/// `Windows\System32\config\SOFTWARE`, and the `HKU\DFT` keys into the Default
/// user's `Users\Default\NTUSER.DAT` (inherited by every account OOBE creates).
pub fn apply_offline_debloat(mount_root: &Path) -> Result<()> {
    let software = crate::fsutil::ci_path(
        mount_root,
        &["Windows", "System32", "config", "SOFTWARE"],
    )
    .context("the applied image has no SOFTWARE hive")?;
    crate::bcd::merge_reg_subtree(&software, DEBLOAT_REG, "HKEY_LOCAL_MACHINE\\SOFTWARE")?;

    let ntuser = crate::fsutil::ci_path(mount_root, &["Users", "Default", "NTUSER.DAT"])
        .context("the applied image has no Default user NTUSER.DAT")?;
    crate::bcd::merge_reg_subtree(&ntuser, DEBLOAT_REG, "HKEY_USERS\\DFT")?;

    emit::log("Applied the debloat profile to the offline SOFTWARE hive and default user profile");
    Ok(())
}

/// Drop the `USBooty\` post-install helper folder straight onto the Default
/// user's Desktop in an already-applied Windows To Go image at `mount_root`, so
/// every account OOBE creates inherits it. The install path stages these on the
/// USB media and xcopies them during `specialize`; WTG copies them in directly.
pub fn write_desktop_helpers_offline(mount_root: &Path) -> Result<()> {
    let default = crate::fsutil::ci_path(mount_root, &["Users", "Default"])
        .context("the applied image has no Default user profile")?;
    write_helpers_into(&default.join("Desktop").join(DESKTOP_HELPERS_DIR))?;
    emit::log("Copied USBooty post-install helpers to the default user's Desktop");
    Ok(())
}

/// Write the embedded [`DESKTOP_HELPERS`] bundle into `dir`, creating it. CRLF
/// line endings keep the `.bat` files readable in Notepad and stop `cmd` from
/// mis-tokenising lone LFs on some Windows builds.
fn write_helpers_into(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for (name, body) in DESKTOP_HELPERS {
        let path = dir.join(name);
        let crlf = body.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(&path, crlf).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Adapt the requested customization for an already-applied image (Windows To
/// Go): drop the flags that have no effect offline, and keep OOBE
/// **interactive**, matching Rufus's default Windows To Go behavior.
///
/// **Why no auto-logon:** on Windows 11 24H2/25H2 the modern OOBE controller
/// (`CloudExperienceHostBroker` / `UserOOBEController`) only marks setup
/// `COMPLETE` once a real first sign-in happens. If we auto-log a user in,
/// OOBE auto-completes without that, the controller sees `accountNeeded = 1`,
/// and on exit it reverts `COMPLETE -> SPECIALIZE_RESEAL_TO_OOBE` and reboots,
/// forever (the WTG reboot loop). Rufus avoids this by NOT auto-logging in: it
/// optionally pre-seeds a local account with an *empty* password that must be
/// changed at first logon, so the interactive first-sign-in still runs. So here
/// we keep `local_account` (rendered Rufus-style by
/// [`oobe::push_oobe_system_wtg`]) but drop `local_account_password` (WTG uses
/// the empty-password scheme, no plaintext password / auto-logon). Harmless,
/// non-blocking settings (computer name, time zone, locale, BitLocker policy,
/// and the user's own `skip_msaccount` / `BypassNRO`) still pass through.
fn offline_setup(setup: &WindowsSetup) -> WindowsSetup {
    WindowsSetup {
        // windowsPE / Setup-only: no Setup runs in WTG.
        bypass_tpm: false,
        bypass_secureboot: false,
        bypass_ram: false,
        bypass_storage: false,
        bypass_cpu: false,
        bypass_disk: false,
        accept_eula: false,
        product_key: None,
        force_edition_picker: false,
        windows_ca_2023: false,
        // Install-media-relative assets, absent on a running WTG system. (For
        // WTG, debloat + desktop helpers are applied straight to the offline
        // image instead; see `windows_to_go::run`.)
        enable_dotnet35: false,
        apply_debloat: false,
        desktop_helpers: false,
        // Disabling adapters breaks Windows To Go on Win 11 24H2: OOBE's
        // update/servicing step needs the network, so with it gone the pending
        // reboot never clears and OOBE loops on "Why did my PC restart?".
        disable_network_during_oobe: false,
        // Keep the requested local-account name (emitted Rufus-style: empty
        // password, password change forced at first logon, no auto-logon), but
        // drop the plaintext password: an injected password would trigger an
        // <AutoLogon>, which bypasses the interactive first-sign-in the modern
        // controller requires and reseal-loops the image (see the doc above).
        local_account_password: None,
        ..setup.clone()
    }
}

// ---- XML primitives ------------------------------------------------------
//
// Shared by the three pass modules. Kept here rather than in a dedicated
// `xml.rs` because the only callers are the sibling pass modules, and the
// surface is small enough that another file would be more navigation than
// it saves.

/// Emit a `<component>` block once per supported processor architecture with
/// identical body content.
pub(super) fn push_component_per_arch(s: &mut String, name: &str, body: &str) {
    for arch in ARCHITECTURES {
        s.push_str(&format!(
            "    <component name=\"{name}\" processorArchitecture=\"{arch}\" \
             publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n"
        ));
        s.push_str(body);
        s.push_str("    </component>\n");
    }
}

/// Emit a `<FirstLogonCommands>` block from a list of `(description, cmd)`
/// pairs. Order is assigned automatically (1-based). No block is written
/// when the list is empty, which keeps the resulting XML free of empty
/// elements when the user has not asked for any first-logon actions.
pub(super) fn push_first_logon_commands(s: &mut String, cmds: &[(&str, &str)]) {
    if cmds.is_empty() {
        return;
    }
    s.push_str("      <FirstLogonCommands>\n");
    for (order, (desc, cmd)) in cmds.iter().enumerate() {
        s.push_str("        <SynchronousCommand wcm:action=\"add\">\n");
        s.push_str(&format!("          <Order>{}</Order>\n", order + 1));
        s.push_str(&format!(
            "          <Description>{}</Description>\n",
            escape(desc)
        ));
        s.push_str(&format!(
            "          <CommandLine>{}</CommandLine>\n",
            escape(cmd)
        ));
        s.push_str("        </SynchronousCommand>\n");
    }
    s.push_str("      </FirstLogonCommands>\n");
}

/// Push a single `<RunSynchronousCommand>` entry. The optional `<Description>`
/// lands in Setup's log, making post-install diagnostics readable.
pub(super) fn push_run_command(s: &mut String, order: usize, cmd: &str, description: Option<&str>) {
    s.push_str("        <RunSynchronousCommand wcm:action=\"add\">\n");
    s.push_str(&format!("          <Order>{order}</Order>\n"));
    if let Some(d) = description {
        s.push_str(&format!(
            "          <Description>{}</Description>\n",
            escape(d)
        ));
    }
    s.push_str(&format!("          <Path>{}</Path>\n", escape(cmd)));
    s.push_str("        </RunSynchronousCommand>\n");
}

/// Trim, drop characters Windows forbids in a hostname, and cap at 15 chars.
pub(super) fn sanitize_computer_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| {
            !c.is_whitespace() && !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .take(15)
        .collect()
}

/// Escape the five XML metacharacters in element text.
pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_setup_is_a_bare_document() {
        let xml = generate(&WindowsSetup::default());
        assert!(xml.contains("<unattend"));
        assert!(!xml.contains("<settings"));
    }

    #[test]
    fn offline_unattend_drops_windows_pe_and_media_assets() {
        let setup = WindowsSetup {
            bypass_tpm: true,         // windowsPE-only → dropped
            enable_dotnet35: true,    // media-relative → dropped
            apply_debloat: true,      // media-relative → dropped
            timezone: Some("UTC".into()),        // specialize → kept
            ..WindowsSetup::default()
        };
        let xml = generate_offline(&setup);
        assert!(!xml.contains("pass=\"windowsPE\""));
        assert!(!xml.contains("sources\\sxs"));
        assert!(!xml.contains("usbooty-debloat.reg"));
        assert!(xml.contains("<TimeZone>UTC</TimeZone>"));
    }

    #[test]
    fn offline_unattend_with_account_uses_rufus_empty_password_scheme() {
        // WTG with a local account, Rufus-style: no Audit reseal, no AutoLogon,
        // an empty-password account in Administrators;Power Users, and a
        // first-logon password change. The plaintext password is dropped (it
        // would force an AutoLogon and reseal-loop the image).
        let setup = WindowsSetup {
            local_account: Some("winpe".into()),
            local_account_password: Some("hunter2".into()),
            computer_name: Some("winpe-pc".into()),
            timezone: Some("UTC".into()),
            ..WindowsSetup::default()
        };
        let xml = generate_offline(&setup);
        // No Audit-mode reseal and no AutoLogon: interactive first-sign-in runs.
        assert!(!xml.contains("<Reseal>"), "WTG must not redirect to Audit mode");
        assert!(!xml.contains("<Mode>Audit</Mode>"));
        assert!(!xml.contains("<AutoLogon>"), "WTG must not auto-logon");
        // Rufus-style account: empty-password blob, Administrators;Power Users.
        assert!(xml.contains("<UserAccounts>"));
        assert!(xml.contains("<Name>winpe</Name>"));
        assert!(xml.contains("<Value>UABhAHMAcwB3AG8AcgBkAA==</Value>"));
        assert!(xml.contains("<Group>Administrators;Power Users</Group>"));
        // The plaintext password must not leak into the answer file.
        assert!(!xml.contains("hunter2"));
        // First-logon hardening (per Rufus). The CommandLine is XML-escaped, so
        // the quotes around the name appear as &quot;.
        assert!(xml.contains("net user &quot;winpe&quot; /logonpasswordchg:yes"));
        assert!(xml.contains("net accounts /maxpwage:unlimited"));
        // Harmless machine settings still apply.
        assert!(xml.contains("<ComputerName>winpe-pc</ComputerName>"));
        assert!(xml.contains("<TimeZone>UTC</TimeZone>"));
        // The online (Setup) path is unaffected: it still honors the password.
        let online = generate(&setup);
        assert!(online.contains("<Name>winpe</Name>"));
        assert!(online.contains("<AutoLogon>"));
    }

    #[test]
    fn offline_unattend_without_account_stays_interactive() {
        // No account requested → accountless interactive OOBE: no UserAccounts,
        // no AutoLogon, no Reseal.
        let setup = WindowsSetup {
            timezone: Some("UTC".into()),
            ..WindowsSetup::default()
        };
        let xml = generate_offline(&setup);
        assert!(!xml.contains("<Reseal>"));
        assert!(!xml.contains("<AutoLogon>"));
        assert!(!xml.contains("<UserAccounts>"));
        assert!(xml.contains("<TimeZone>UTC</TimeZone>"));
    }

    #[test]
    fn offline_unattend_honors_skip_msaccount_oobe_flags() {
        // The user's chosen OOBE hide flags still flow into the offline file so
        // the modern Win 11 forced-MS-account screen can be bypassed: BypassNRO
        // (specialize) + HideOnlineAccountScreens (oobeSystem).
        let setup = WindowsSetup {
            skip_msaccount: true,
            ..WindowsSetup::default()
        };
        let xml = generate_offline(&setup);
        assert!(xml.contains("BypassNRO"));
        assert!(xml.contains("<HideOnlineAccountScreens>true</HideOnlineAccountScreens>"));
        assert!(!xml.contains("<Reseal>"));
    }

    #[test]
    fn offline_unattend_rejects_reserved_account_name() {
        // A reserved name (e.g. "Administrator") must not produce a LocalAccount.
        let setup = WindowsSetup {
            local_account: Some("Administrator".into()),
            ..WindowsSetup::default()
        };
        let xml = generate_offline(&setup);
        assert!(!xml.contains("<UserAccounts>"));
    }

    #[test]
    fn each_component_is_emitted_for_three_architectures() {
        let setup = WindowsSetup {
            bypass_tpm: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        // The Win 11 bypass component should appear three times, once each
        // for x86, arm64, and amd64.
        assert_eq!(
            xml.matches("name=\"Microsoft-Windows-Setup\"").count(),
            3,
            "expected one Microsoft-Windows-Setup component per architecture"
        );
        for arch in ["x86", "arm64", "amd64"] {
            let needle = format!("processorArchitecture=\"{arch}\"");
            assert!(
                xml.contains(&needle),
                "missing component variant for {arch}"
            );
        }
    }

    #[test]
    fn bypass_flags_emit_all_six_labconfig_writes() {
        let setup = WindowsSetup {
            bypass_tpm: true,
            bypass_secureboot: true,
            bypass_ram: true,
            bypass_storage: true,
            bypass_cpu: true,
            bypass_disk: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        for key in [
            "BypassTPMCheck",
            "BypassSecureBootCheck",
            "BypassStorageCheck",
            "BypassCPUCheck",
            "BypassRAMCheck",
            "BypassDiskCheck",
        ] {
            // Each key appears in every architecture-specific component → 3×.
            assert_eq!(
                xml.matches(key).count(),
                3,
                "expected {key} once per architecture"
            );
        }
    }

    #[test]
    fn skip_msaccount_emits_both_bypassnro_and_hideonlineaccountscreens() {
        let setup = WindowsSetup {
            skip_msaccount: true,
            local_account: Some("Tom".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("BypassNRO"));
        assert!(xml.contains("<HideOnlineAccountScreens>true</HideOnlineAccountScreens>"));
        assert!(xml.contains("<Name>Tom</Name>"));
    }

    #[test]
    fn disable_network_pairs_with_firstlogon_re_enable() {
        let setup = WindowsSetup {
            disable_network_during_oobe: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("Get-NetAdapter | Disable-NetAdapter"));
        assert!(xml.contains("<FirstLogonCommands>"));
        assert!(xml.contains("Get-NetAdapter | Enable-NetAdapter"));
        // The disable runs in specialize; the enable runs in oobeSystem.
        let disable_at = xml.find("Disable-NetAdapter").unwrap();
        let enable_at = xml.find("Enable-NetAdapter").unwrap();
        assert!(disable_at < enable_at);
    }

    #[test]
    fn dotnet35_enables_netfx3_via_dism() {
        let setup = WindowsSetup {
            enable_dotnet35: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("/FeatureName:NetFx3"));
        assert!(xml.contains("sources\\sxs"));
    }

    #[test]
    fn hide_oem_registration_adds_oobe_element() {
        let setup = WindowsSetup {
            hide_oem_registration: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("<HideOEMRegistrationScreen>true</HideOEMRegistrationScreen>"));
    }

    #[test]
    fn network_location_work_adds_oobe_element() {
        let setup = WindowsSetup {
            network_location_work: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("<NetworkLocation>Work</NetworkLocation>"));
    }

    #[test]
    fn password_emits_autologon_and_local_account_password() {
        let setup = WindowsSetup {
            local_account: Some("ada".into()),
            local_account_password: Some("hunter2".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("<AutoLogon>"));
        assert!(xml.contains("<Username>ada</Username>"));
        // The password appears twice (AutoLogon, LocalAccount) per architecture.
        assert_eq!(xml.matches("<Value>hunter2</Value>").count(), 2 * 3);
    }

    #[test]
    fn local_account_without_password_does_not_emit_autologon() {
        let setup = WindowsSetup {
            local_account: Some("ada".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(!xml.contains("<AutoLogon>"));
        assert!(xml.contains("<Name>ada</Name>"));
    }

    #[test]
    fn computer_name_is_sanitized_and_truncated() {
        let setup = WindowsSetup {
            computer_name: Some("My Box/Name?".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("<ComputerName>MyBoxName</ComputerName>"));
        assert_eq!(
            sanitize_computer_name("0123456789ABCDEF-extra"),
            "0123456789ABCDE"
        );
    }

    #[test]
    fn locale_lands_in_both_windowspe_and_oobesystem() {
        let setup = WindowsSetup {
            locale: Some("fr-FR".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("Microsoft-Windows-International-Core-WinPE"));
        assert!(xml.contains("name=\"Microsoft-Windows-International-Core\""));
        assert!(xml.contains("<UserLocale>fr-FR</UserLocale>"));
    }

    #[test]
    fn product_key_and_accept_eula_share_one_user_data_block_per_arch() {
        let setup = WindowsSetup {
            product_key: Some("ABCDE-12345-FGHIJ-67890-KLMNO".into()),
            accept_eula: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        // One UserData per component variant, three architectures.
        assert_eq!(xml.matches("<UserData>").count(), 3);
        assert!(xml.contains("<Key>ABCDE-12345-FGHIJ-67890-KLMNO</Key>"));
        assert!(xml.contains("<AcceptEula>true</AcceptEula>"));
    }

    #[test]
    fn timezone_lands_in_specialize_shell_setup() {
        let setup = WindowsSetup {
            timezone: Some("Romance Standard Time".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("pass=\"specialize\""));
        assert!(xml.contains("<TimeZone>Romance Standard Time</TimeZone>"));
    }

    #[test]
    fn debloat_emits_reg_load_import_unload_in_order() {
        let setup = WindowsSetup {
            apply_debloat: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        let load = xml.find("reg load HKU\\DFT").expect("load present");
        let import = xml.find("usbooty-debloat.reg").expect("import present");
        let unload = xml.find("reg unload HKU\\DFT").expect("unload present");
        assert!(
            load < import && import < unload,
            "commands must run in order"
        );
    }

    #[test]
    fn hide_wireless_setup_emits_oobe_element() {
        let setup = WindowsSetup {
            hide_wireless_setup: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("<HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>"));
    }

    #[test]
    fn desktop_helpers_xcopies_to_default_users_desktop() {
        let setup = WindowsSetup {
            desktop_helpers: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("xcopy"));
        assert!(xml.contains("USBooty"));
        assert!(xml.contains("C:\\Users\\Default\\Desktop\\USBooty"));
        // Sentinel guards against false-positive matches on non-USB drives.
        assert!(xml.contains("1-Win11Debloat.bat"));
    }

    #[test]
    fn force_edition_picker_writes_sources_ei_cfg() {
        let dir = tempfile::tempdir().expect("tempdir");
        let setup = WindowsSetup {
            force_edition_picker: true,
            ..WindowsSetup::default()
        };
        write(dir.path(), &setup).expect("write");
        // Path joining uses the assets-module constants so any rename of
        // the directory or filename is caught here automatically.
        let path = dir
            .path()
            .join(assets::EI_CFG_DIR)
            .join(assets::EI_CFG_NAME);
        let cfg = std::fs::read_to_string(&path).expect("ei.cfg");
        // No `[EditionID]` block at all: Setup falls through to its
        // built-in picker rather than silently picking Home from the
        // firmware MSDM key.
        assert!(!cfg.contains("[EditionID]"));
        assert!(cfg.contains("[Channel]\n_Default"));
        assert!(cfg.contains("[VL]\n0"));
    }

    #[test]
    fn force_edition_picker_unset_leaves_no_ei_cfg() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), &WindowsSetup::default()).expect("write");
        let path = dir
            .path()
            .join(assets::EI_CFG_DIR)
            .join(assets::EI_CFG_NAME);
        assert!(!path.exists());
    }

    #[test]
    fn desktop_helpers_bundle_lists_every_shipped_script() {
        // Sanity: any change to the bundled scripts should keep the list
        // intact. If you add or rename a script, update both the constant
        // and this expectation.
        let names: Vec<&str> = DESKTOP_HELPERS.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            vec![
                "1-Win11Debloat.bat",
                "2-ChrisTitus-Winutil.bat",
                "2.1-ChrisTitus-Winutil-Dev.bat",
                "3-Massgravel-Activator.bat",
                "4-Remove-OneDrive.bat",
                "5-OfficeTool.bat",
                "6-Install-Chocolatey.bat",
                "7-Install-Scoop.bat",
                "8-Install-Winget.bat",
                "9-Remove-Windows-AI.bat",
                "10-Winhance.bat",
                "11-FR33THY-Ultimate.bat",
                "12-Install-PowerToys.bat",
                "13-Disable-FastStartup.bat",
                "14-Enable-LongPaths.bat",
                "15-Install-VCRedist.bat",
                "16-Install-DirectX.bat",
                "17-Install-Browser.bat",
                "README.txt",
            ]
        );
    }
}
