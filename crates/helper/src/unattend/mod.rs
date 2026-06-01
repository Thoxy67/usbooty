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

/// Write the embedded [`DESKTOP_HELPERS`] bundle into `dir`, creating it. CRLF
/// line endings keep the `.bat` files readable in Notepad and stop `cmd` from
/// mis-tokenising lone LFs on some Windows builds.
fn write_helpers_into(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for (name, body) in DESKTOP_HELPERS {
        // `name` is a category-subfolder-relative path (e.g.
        // "1 Debloat & Privacy/1-Win11Debloat.bat"); create the parent folder
        // before writing the script into it.
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let crlf = body.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(&path, crlf).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

// ---- XML primitives ------------------------------------------------------
//
// Shared by the three pass modules. Kept here rather than in a dedicated
// `xml.rs` because the only callers are the sibling pass modules, and the
// surface is small enough that another file would be more navigation than
// it saves.

/// The processor architectures to emit `<component>` blocks for. When the
/// image's arch is known (and supported) we emit just that one, cutting the
/// unattend to a third of its size; otherwise we emit every supported arch,
/// which is always safe since Windows ignores components whose arch doesn't
/// match the running image.
pub(super) fn target_archs(setup: &WindowsSetup) -> Vec<&'static str> {
    if let Some(arch) = setup.arch.as_deref() {
        if let Some(&matched) = ARCHITECTURES.iter().find(|&&a| a == arch) {
            return vec![matched];
        }
    }
    ARCHITECTURES.to_vec()
}

/// Emit a `<component>` block for each architecture in `archs` with identical
/// body content.
pub(super) fn push_component_per_arch(s: &mut String, archs: &[&str], name: &str, body: &str) {
    for arch in archs {
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
    fn known_arch_emits_a_single_component() {
        let setup = WindowsSetup {
            bypass_tpm: true,
            arch: Some("amd64".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        // Only the amd64 component is emitted, not all three.
        assert_eq!(xml.matches("name=\"Microsoft-Windows-Setup\"").count(), 1);
        assert!(xml.contains("processorArchitecture=\"amd64\""));
        assert!(!xml.contains("processorArchitecture=\"x86\""));
        assert!(!xml.contains("processorArchitecture=\"arm64\""));
    }

    #[test]
    fn unknown_arch_falls_back_to_all_architectures() {
        let setup = WindowsSetup {
            bypass_tpm: true,
            arch: Some("sparc".into()), // not in ARCHITECTURES
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert_eq!(xml.matches("name=\"Microsoft-Windows-Setup\"").count(), 3);
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
        // Sentinel (a top-level file) guards against false-positive matches on
        // non-USB drives; it lives at the USBooty root, not in a subfolder.
        assert!(xml.contains("README.txt"));
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
                "1 Debloat & Privacy/Win11Debloat.bat",
                "1 Debloat & Privacy/ChrisTitus-Winutil.bat",
                "1 Debloat & Privacy/ChrisTitus-Winutil-Dev.bat",
                "1 Debloat & Privacy/Remove-OneDrive.bat",
                "1 Debloat & Privacy/Remove-Windows-AI.bat",
                "1 Debloat & Privacy/Winhance.bat",
                "2 Tweaks & Performance/FR33THY-Ultimate.bat",
                "2 Tweaks & Performance/Disable-FastStartup.bat",
                "2 Tweaks & Performance/Enable-LongPaths.bat",
                "2 Tweaks & Performance/Disable-GameBar-GameDVR.bat",
                "2 Tweaks & Performance/Enable-GPU-Scheduling.bat",
                "2 Tweaks & Performance/Enable-Ultimate-Performance.bat",
                "2 Tweaks & Performance/Disable-Hibernation.bat",
                "2 Tweaks & Performance/Enable-GodMode.bat",
                "2 Tweaks & Performance/Restore-Classic-ContextMenu.bat",
                "3 Install Apps/OfficeTool.bat",
                "3 Install Apps/Install-PowerToys.bat",
                "3 Install Apps/Install-VCRedist.bat",
                "3 Install Apps/Install-DirectX.bat",
                "3 Install Apps/Install-Browser.bat",
                "3 Install Apps/Install-DotNet-Runtimes.bat",
                "4 Package Managers/Install-Chocolatey.bat",
                "4 Package Managers/Install-Scoop.bat",
                "4 Package Managers/Install-Winget.bat",
                "5 Activation/Massgravel-Activator.bat",
                "README.txt",
            ]
        );
    }
}
