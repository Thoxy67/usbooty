//! Generating a Windows `autounattend.xml` to customize the installation.
//!
//! The file is written to the root of the USB; Windows Setup reads it
//! automatically. The Windows 11 hardware-check bypasses go in the `windowsPE`
//! pass as `LabConfig` registry writes — the same approach Rufus uses, needing
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

use anyhow::{Context, Result};
use std::path::Path;

use usbooty_core::WindowsSetup;

use crate::emit;

/// The vendored debloat policy, written next to `autounattend.xml` when
/// [`WindowsSetup::apply_debloat`] is set.
const DEBLOAT_REG: &str = include_str!("debloat.reg");

/// The filename of the debloat policy on the USB root. The autounattend's
/// `specialize`-pass `for` loop scans D..Z for this exact name.
const DEBLOAT_REG_NAME: &str = "usbooty-debloat.reg";

/// Name of the folder on the USB that holds the post-install `.bat`
/// helpers. The `specialize`-pass `for` loop scans drive letters for
/// `<letter>:\<DESKTOP_HELPERS_DIR>\<DESKTOP_HELPERS_SENTINEL>` to find
/// the install media and xcopies the whole folder to the Default user's
/// Desktop so every new account inherits it.
const DESKTOP_HELPERS_DIR: &str = "USBooty";
const DESKTOP_HELPERS_SENTINEL: &str = "1-Win11Debloat.bat";

/// Each entry is `(filename, contents)`. The files are written verbatim
/// into `<mount>/USBooty/` when [`WindowsSetup::desktop_helpers`] is set.
const DESKTOP_HELPERS: &[(&str, &str)] = &[
    (
        "1-Win11Debloat.bat",
        include_str!("desktop_helpers/1-Win11Debloat.bat"),
    ),
    (
        "2-ChrisTitus-Winutil.bat",
        include_str!("desktop_helpers/2-ChrisTitus-Winutil.bat"),
    ),
    (
        "2.1-ChrisTitus-Winutil-Dev.bat",
        include_str!("desktop_helpers/2.1-ChrisTitus-Winutil-Dev.bat"),
    ),
    (
        "3-Massgravel-Activator.bat",
        include_str!("desktop_helpers/3-Massgravel-Activator.bat"),
    ),
    (
        "4-Remove-OneDrive.bat",
        include_str!("desktop_helpers/4-Remove-OneDrive.bat"),
    ),
    (
        "5-OfficeTool.bat",
        include_str!("desktop_helpers/5-OfficeTool.bat"),
    ),
    (
        "6-Install-Chocolatey.bat",
        include_str!("desktop_helpers/6-Install-Chocolatey.bat"),
    ),
    (
        "7-Install-Scoop.bat",
        include_str!("desktop_helpers/7-Install-Scoop.bat"),
    ),
    (
        "8-Install-Winget.bat",
        include_str!("desktop_helpers/8-Install-Winget.bat"),
    ),
    (
        "9-Remove-Windows-AI.bat",
        include_str!("desktop_helpers/9-Remove-Windows-AI.bat"),
    ),
    (
        "10-Winhance.bat",
        include_str!("desktop_helpers/10-Winhance.bat"),
    ),
    (
        "11-FR33THY-Ultimate.bat",
        include_str!("desktop_helpers/11-FR33THY-Ultimate.bat"),
    ),
    ("README.txt", include_str!("desktop_helpers/README.txt")),
];

/// Supported processor architectures. Windows Setup matches `<component>` by
/// architecture, so emitting only one would silently skip ARM and x86 hosts.
const ARCHITECTURES: [&str; 3] = ["x86", "arm64", "amd64"];

/// PowerShell one-liner that scans drive letters C..K for the install media's
/// `sources\sxs` folder and enables the .NET Framework 3.5 component from
/// there. The first match wins; missing drives no-op.
const DOTNET35_COMMAND: &str = concat!(
    r#"powershell.exe -NoProfile -WindowStyle Hidden -Command ""#,
    r#"foreach($d in 'C','D','E','F','G','H','I','J','K'){"#,
    r#"$src=Join-Path ($d+':') 'sources\sxs';"#,
    r#"if(Test-Path $src\*.cab){"#,
    r#"dism /Online /Enable-Feature /FeatureName:NetFx3 /All /LimitAccess /Source:$src;"#,
    r#"break}}""#,
);

/// PowerShell command that disables every network adapter — used in the
/// `specialize` pass to force OOBE down the local-account path on Win 11 24H2+.
const DISABLE_ADAPTERS_COMMAND: &str = concat!(
    r#"powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass "#,
    r#"-Command "Get-NetAdapter | Disable-NetAdapter -Confirm:$false""#,
);

/// PowerShell command that re-enables every network adapter on first logon.
const ENABLE_ADAPTERS_COMMAND: &str = concat!(
    r#"powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass "#,
    r#"-Command "Get-NetAdapter | Enable-NetAdapter -Confirm:$false""#,
);

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
        // Lay out `<mount>/USBooty/{bat,readme}`. CRLF line endings keep
        // Notepad readable and stop cmd from choking on lone LF, which
        // some Windows builds tokenise unpredictably in batch files.
        let dir = mount.join(DESKTOP_HELPERS_DIR);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating {}", dir.display()))?;
        for (name, body) in DESKTOP_HELPERS {
            let path = dir.join(name);
            let crlf = body.replace("\r\n", "\n").replace('\n', "\r\n");
            std::fs::write(&path, crlf)
                .with_context(|| format!("writing {}", path.display()))?;
        }
        emit::log("Wrote USBooty/ post-install helpers next to autounattend.xml");
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
    push_windows_pe(&mut s, setup);
    push_specialize(&mut s, setup);
    push_oobe_system(&mut s, setup);
    s.push_str("</unattend>\n");
    s
}

/// `windowsPE` pass — Setup-time settings: the Win 11 LabConfig bypasses, the
/// product key and EULA accept, and the Setup-UI / system locale.
fn push_windows_pe(s: &mut String, setup: &WindowsSetup) {
    let bypasses: Vec<&str> = [
        setup.bypass_tpm.then_some("BypassTPMCheck"),
        setup.bypass_secureboot.then_some("BypassSecureBootCheck"),
        setup.bypass_storage.then_some("BypassStorageCheck"),
        setup.bypass_cpu.then_some("BypassCPUCheck"),
        setup.bypass_ram.then_some("BypassRAMCheck"),
        setup.bypass_disk.then_some("BypassDiskCheck"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let product_key = setup.product_key.as_deref().filter(|k| !k.is_empty());
    let has_user_data = setup.accept_eula || product_key.is_some();
    let has_setup_component = has_user_data || !bypasses.is_empty();
    let setup_locale = setup.locale.as_deref().filter(|l| !l.is_empty());

    if !has_setup_component && setup_locale.is_none() {
        return;
    }

    let mut setup_body = String::new();
    if has_user_data {
        setup_body.push_str("      <UserData>\n");
        if let Some(key) = product_key {
            setup_body.push_str("        <ProductKey>\n");
            setup_body.push_str(&format!("          <Key>{}</Key>\n", escape(key)));
            setup_body.push_str("        </ProductKey>\n");
        }
        if setup.accept_eula {
            setup_body.push_str("        <AcceptEula>true</AcceptEula>\n");
        }
        setup_body.push_str("      </UserData>\n");
    }
    if !bypasses.is_empty() {
        setup_body.push_str("      <RunSynchronous>\n");
        for (i, name) in bypasses.iter().enumerate() {
            let cmd = format!(
                "reg.exe add \"HKLM\\SYSTEM\\Setup\\LabConfig\" \
                 /v {name} /t REG_DWORD /d 1 /f"
            );
            let desc = (i == 0).then_some("Skip Windows 11 hardware-requirement checks");
            push_run_command(&mut setup_body, i + 1, &cmd, desc);
        }
        setup_body.push_str("      </RunSynchronous>\n");
    }

    let mut intl_body = String::new();
    if let Some(loc) = setup_locale {
        let loc = escape(loc);
        intl_body.push_str("      <SetupUILanguage>\n");
        intl_body.push_str(&format!("        <UILanguage>{loc}</UILanguage>\n"));
        intl_body.push_str("      </SetupUILanguage>\n");
        intl_body.push_str(&format!("      <InputLocale>{loc}</InputLocale>\n"));
        intl_body.push_str(&format!("      <SystemLocale>{loc}</SystemLocale>\n"));
        intl_body.push_str(&format!("      <UILanguage>{loc}</UILanguage>\n"));
    }

    s.push_str("  <settings pass=\"windowsPE\">\n");
    if !setup_body.is_empty() {
        push_component_per_arch(s, "Microsoft-Windows-Setup", &setup_body);
    }
    if !intl_body.is_empty() {
        push_component_per_arch(s, "Microsoft-Windows-International-Core-WinPE", &intl_body);
    }
    s.push_str("  </settings>\n");
}

/// `specialize` pass — post-image, pre-OOBE machine settings: the BypassNRO
/// fallback, the no-network-during-OOBE trick, the .NET 3.5 enabler, the
/// computer name, the time zone, and the debloat-policy import.
fn push_specialize(s: &mut String, setup: &WindowsSetup) {
    let mut deploy_cmds: Vec<(String, Option<&'static str>)> = Vec::new();
    if setup.skip_msaccount {
        // Win 10 + Win 11 pre-24H2 fallback; oobeSystem's
        // `HideOnlineAccountScreens` covers Win 11 24H2+.
        deploy_cmds.push((
            "reg.exe add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\OOBE\" \
             /v BypassNRO /t REG_DWORD /d 1 /f"
                .to_string(),
            Some("Allow local-account creation during OOBE (Win 10 / pre-24H2)"),
        ));
    }
    if setup.disable_network_during_oobe {
        // With every adapter offline, OOBE has no internet so it falls back to
        // local-account creation even on builds where the registry / OOBE flags
        // above are ignored. The FirstLogonCommands block re-enables adapters.
        deploy_cmds.push((
            DISABLE_ADAPTERS_COMMAND.to_string(),
            Some("Disable network adapters so OOBE skips Microsoft-account creation"),
        ));
    }
    if setup.enable_dotnet35 {
        deploy_cmds.push((
            DOTNET35_COMMAND.to_string(),
            Some("Enable .NET Framework 3.5 from the install media's sources\\sxs"),
        ));
    }
    if setup.disable_bitlocker {
        // Set the registry guard *before* Windows reaches the OOBE phase
        // where 24H2's automatic device encryption decision happens. The
        // value is also valid (and harmless) on older versions that never
        // auto-encrypt, so we don't need a Win-11-only gate.
        deploy_cmds.push((
            "reg add HKLM\\SYSTEM\\CurrentControlSet\\Control\\BitLocker \
             /v PreventDeviceEncryption /t REG_DWORD /d 1 /f"
                .to_string(),
            Some("Disable Windows automatic BitLocker device encryption"),
        ));
    }
    if setup.apply_debloat {
        deploy_cmds.push((
            "reg load HKU\\DFT C:\\Users\\Default\\NTUSER.DAT".to_string(),
            Some("Mount the default user's hive for per-user debloat policies"),
        ));
        // The USB drive letter is unpredictable on a freshly-installing system,
        // so scan D..Z for the .reg next to autounattend.xml.
        deploy_cmds.push((
            format!(
                "cmd /c \"for %d in (D E F G H I J K L M N O P Q R S T U V W X Y Z) \
                 do if exist %d:\\{name} reg import %d:\\{name}\"",
                name = DEBLOAT_REG_NAME,
            ),
            Some("Import usbooty-debloat.reg from the USB"),
        ));
        deploy_cmds.push((
            "reg unload HKU\\DFT".to_string(),
            Some("Unmount the default user's hive"),
        ));
    }
    if setup.desktop_helpers {
        // xcopy /E (recurse) /I (treat dest as folder, no prompt) /Y
        // (overwrite without prompting) /Q (don't echo filenames). The
        // `for %d` scan handles the unpredictable USB drive letter at
        // specialize time; the sentinel file guards against a match on
        // some other drive that happens to contain a USBooty folder.
        // Destination is Default's Desktop so every new user account
        // created by OOBE inherits the folder on first sign-in.
        deploy_cmds.push((
            format!(
                "cmd /c \"for %d in (D E F G H I J K L M N O P Q R S T U V W X Y Z) \
                 do if exist %d:\\{dir}\\{sentinel} \
                 xcopy /E /I /Y /Q %d:\\{dir} \"C:\\Users\\Default\\Desktop\\{dir}\\\" \"",
                dir = DESKTOP_HELPERS_DIR,
                sentinel = DESKTOP_HELPERS_SENTINEL,
            ),
            Some("Copy USBooty post-install helpers to Default user's Desktop"),
        ));
    }

    let computer_name = setup
        .computer_name
        .as_deref()
        .map(sanitize_computer_name)
        .filter(|n| !n.is_empty());
    let timezone = setup.timezone.as_deref().filter(|t| !t.is_empty());

    if deploy_cmds.is_empty() && computer_name.is_none() && timezone.is_none() {
        return;
    }

    let mut deploy_body = String::new();
    if !deploy_cmds.is_empty() {
        deploy_body.push_str("      <RunSynchronous>\n");
        for (i, (cmd, desc)) in deploy_cmds.iter().enumerate() {
            push_run_command(&mut deploy_body, i + 1, cmd, *desc);
        }
        deploy_body.push_str("      </RunSynchronous>\n");
    }

    let mut shell_body = String::new();
    if let Some(name) = computer_name {
        shell_body.push_str(&format!(
            "      <ComputerName>{}</ComputerName>\n",
            escape(&name)
        ));
    }
    if let Some(tz) = timezone {
        shell_body.push_str(&format!("      <TimeZone>{}</TimeZone>\n", escape(tz)));
    }

    s.push_str("  <settings pass=\"specialize\">\n");
    if !deploy_body.is_empty() {
        push_component_per_arch(s, "Microsoft-Windows-Deployment", &deploy_body);
    }
    if !shell_body.is_empty() {
        push_component_per_arch(s, "Microsoft-Windows-Shell-Setup", &shell_body);
    }
    s.push_str("  </settings>\n");
}

/// `oobeSystem` pass — first-boot user-facing settings: which OOBE prompts to
/// hide, the auto-logon + local account, FirstLogonCommands that re-enable
/// the network, and the user-facing locale.
fn push_oobe_system(s: &mut String, setup: &WindowsSetup) {
    let oobe_items = build_oobe_items(setup);
    let account = setup
        .local_account
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let password = setup
        .local_account_password
        .as_deref()
        .filter(|p| !p.is_empty());
    let has_autologon = account.is_some() && password.is_some();
    let oobe_locale = setup.locale.as_deref().filter(|l| !l.is_empty());

    let has_shell = !oobe_items.is_empty()
        || has_autologon
        || account.is_some()
        || setup.disable_network_during_oobe;
    if !has_shell && oobe_locale.is_none() {
        return;
    }

    let mut shell_body = String::new();
    // Shell-Setup schema order: AutoLogon → FirstLogonCommands → OOBE → UserAccounts.
    if let (Some(name), Some(pass)) = (account, password) {
        shell_body.push_str("      <AutoLogon>\n");
        shell_body.push_str(&format!("        <Username>{}</Username>\n", escape(name)));
        shell_body.push_str("        <Password>\n");
        shell_body.push_str(&format!("          <Value>{}</Value>\n", escape(pass)));
        shell_body.push_str("          <PlainText>true</PlainText>\n");
        shell_body.push_str("        </Password>\n");
        shell_body.push_str("        <Enabled>true</Enabled>\n");
        shell_body.push_str("        <LogonCount>1</LogonCount>\n");
        shell_body.push_str("      </AutoLogon>\n");
    }
    // Build the FirstLogonCommands block as a table of (description,
    // command-line) pairs so adding a future first-logon action is a
    // single row in this Vec instead of another copy of the eight-line
    // SynchronousCommand template.
    let mut first_logon: Vec<(&'static str, &str)> = Vec::new();
    if setup.disable_network_during_oobe {
        first_logon.push((
            "Re-enable network adapters after OOBE",
            ENABLE_ADAPTERS_COMMAND,
        ));
    }
    push_first_logon_commands(&mut shell_body, &first_logon);
    if !oobe_items.is_empty() {
        shell_body.push_str("      <OOBE>\n");
        for (name, value) in &oobe_items {
            shell_body.push_str(&format!("        <{name}>{value}</{name}>\n"));
        }
        shell_body.push_str("      </OOBE>\n");
    }
    if let Some(name) = account {
        shell_body.push_str("      <UserAccounts>\n        <LocalAccounts>\n");
        shell_body.push_str("          <LocalAccount wcm:action=\"add\">\n");
        if let Some(pass) = password {
            shell_body.push_str("            <Password>\n");
            shell_body.push_str(&format!("              <Value>{}</Value>\n", escape(pass)));
            shell_body.push_str("              <PlainText>true</PlainText>\n");
            shell_body.push_str("            </Password>\n");
        }
        shell_body.push_str(&format!("            <Name>{}</Name>\n", escape(name)));
        shell_body.push_str(&format!(
            "            <DisplayName>{}</DisplayName>\n",
            escape(name)
        ));
        shell_body.push_str("            <Group>Administrators</Group>\n");
        shell_body.push_str("          </LocalAccount>\n");
        shell_body.push_str("        </LocalAccounts>\n      </UserAccounts>\n");
    }

    let mut intl_body = String::new();
    if let Some(loc) = oobe_locale {
        let loc = escape(loc);
        intl_body.push_str(&format!("      <InputLocale>{loc}</InputLocale>\n"));
        intl_body.push_str(&format!("      <SystemLocale>{loc}</SystemLocale>\n"));
        intl_body.push_str(&format!("      <UILanguage>{loc}</UILanguage>\n"));
        intl_body.push_str(&format!("      <UserLocale>{loc}</UserLocale>\n"));
    }

    s.push_str("  <settings pass=\"oobeSystem\">\n");
    if !shell_body.is_empty() {
        push_component_per_arch(s, "Microsoft-Windows-Shell-Setup", &shell_body);
    }
    if !intl_body.is_empty() {
        push_component_per_arch(s, "Microsoft-Windows-International-Core", &intl_body);
    }
    s.push_str("  </settings>\n");
}

/// Collect every requested `<OOBE>` child element, in roughly alphabetical
/// (schema) order. Each entry is `(element_name, text_value)`.
fn build_oobe_items(setup: &WindowsSetup) -> Vec<(&'static str, &'static str)> {
    let mut items = Vec::new();
    if setup.disable_telemetry {
        items.push(("HideEULAPage", "true"));
    }
    if setup.hide_oem_registration {
        items.push(("HideOEMRegistrationScreen", "true"));
    }
    if setup.skip_msaccount {
        // The supported Win 11 24H2+ way to bypass forced MS-account creation;
        // older Windows ignore the element.
        items.push(("HideOnlineAccountScreens", "true"));
    }
    if setup.hide_wireless_setup {
        items.push(("HideWirelessSetupInOOBE", "true"));
    }
    if setup.network_location_work {
        items.push(("NetworkLocation", "Work"));
    }
    if setup.disable_telemetry {
        items.push(("ProtectYourPC", "3"));
    }
    items
}

/// Emit a `<component>` block once per supported processor architecture with
/// identical body content.
fn push_component_per_arch(s: &mut String, name: &str, body: &str) {
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
fn push_first_logon_commands(s: &mut String, cmds: &[(&str, &str)]) {
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
fn push_run_command(s: &mut String, order: usize, cmd: &str, description: Option<&str>) {
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
fn sanitize_computer_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| {
            !c.is_whitespace() && !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .take(15)
        .collect()
}

/// Escape the five XML metacharacters in element text.
fn escape(text: &str) -> String {
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
        // The Win 11 bypass component should appear three times — once each
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
        // One UserData per component variant — three architectures.
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
                "README.txt",
            ]
        );
    }
}
