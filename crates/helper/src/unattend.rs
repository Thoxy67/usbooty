//! Generating a Windows `autounattend.xml` to customize the installation.
//!
//! The file is written to the root of the USB; Windows Setup reads it
//! automatically. The Windows 11 hardware-check bypasses go in the `windowsPE`
//! pass as `LabConfig` registry writes — the same approach Rufus uses, needing
//! no offline editing of the boot image.

use anyhow::{Context, Result};
use std::path::Path;

use usbooty_core::WindowsSetup;

use crate::emit;

/// Standard component attributes for an `amd64` unattend component.
const COMP: &str = concat!(
    r#"processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" "#,
    r#"language="neutral" versionScope="nonSxS""#,
);

/// Write `autounattend.xml` for `setup` into the root of the mounted target.
pub fn write(mount: &Path, setup: &WindowsSetup) -> Result<()> {
    let path = mount.join("autounattend.xml");
    std::fs::write(&path, generate(setup))
        .with_context(|| format!("writing {}", path.display()))?;
    emit::log("Applied Windows customization (autounattend.xml)");
    Ok(())
}

/// Build the `autounattend.xml` document for the requested customizations.
pub fn generate(setup: &WindowsSetup) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str(
        "<unattend xmlns=\"urn:schemas-microsoft-com:unattend\" \
         xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n",
    );

    // windowsPE: bypass the Windows 11 hardware checks via LabConfig.
    let mut checks: Vec<&str> = Vec::new();
    if setup.bypass_tpm {
        checks.push("BypassTPMCheck");
    }
    if setup.bypass_secureboot {
        checks.push("BypassSecureBootCheck");
    }
    if setup.bypass_ram {
        checks.push("BypassRAMCheck");
    }
    if !checks.is_empty() {
        let cmds: Vec<String> = checks
            .iter()
            .map(|key| {
                format!("reg add HKLM\\SYSTEM\\Setup\\LabConfig /v {key} /t REG_DWORD /d 1 /f")
            })
            .collect();
        push_run_synchronous(&mut s, "windowsPE", "Microsoft-Windows-Setup", &cmds);
    }

    // specialize: skip the forced Microsoft-account requirement in OOBE.
    if setup.skip_msaccount {
        let cmd = "reg add HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\OOBE \
                   /v BypassNRO /t REG_DWORD /d 1 /f"
            .to_string();
        push_run_synchronous(
            &mut s,
            "specialize",
            "Microsoft-Windows-Deployment",
            &[cmd],
        );
    }

    // oobeSystem: telemetry prompt + optional local account.
    if setup.disable_telemetry || setup.local_account.is_some() {
        s.push_str("  <settings pass=\"oobeSystem\">\n");
        s.push_str(&format!(
            "    <component name=\"Microsoft-Windows-Shell-Setup\" {COMP}>\n"
        ));
        if setup.disable_telemetry {
            s.push_str("      <OOBE>\n");
            s.push_str("        <HideEULAPage>true</HideEULAPage>\n");
            s.push_str("        <ProtectYourPC>3</ProtectYourPC>\n");
            s.push_str("      </OOBE>\n");
        }
        if let Some(name) = setup.local_account.as_deref().filter(|n| !n.is_empty()) {
            let name = escape(name);
            s.push_str("      <UserAccounts>\n        <LocalAccounts>\n");
            s.push_str("          <LocalAccount wcm:action=\"add\">\n");
            s.push_str(&format!("            <Name>{name}</Name>\n"));
            s.push_str(&format!("            <DisplayName>{name}</DisplayName>\n"));
            s.push_str("            <Group>Administrators</Group>\n");
            s.push_str("          </LocalAccount>\n");
            s.push_str("        </LocalAccounts>\n      </UserAccounts>\n");
        }
        s.push_str("    </component>\n  </settings>\n");
    }

    s.push_str("</unattend>\n");
    s
}

/// Append a `<settings>` block running `cmds` synchronously in `pass`.
fn push_run_synchronous(s: &mut String, pass: &str, component: &str, cmds: &[String]) {
    s.push_str(&format!("  <settings pass=\"{pass}\">\n"));
    s.push_str(&format!("    <component name=\"{component}\" {COMP}>\n"));
    s.push_str("      <RunSynchronous>\n");
    for (i, cmd) in cmds.iter().enumerate() {
        s.push_str("        <RunSynchronousCommand wcm:action=\"add\">\n");
        s.push_str(&format!("          <Order>{}</Order>\n", i + 1));
        s.push_str(&format!("          <Path>{}</Path>\n", escape(cmd)));
        s.push_str("        </RunSynchronousCommand>\n");
    }
    s.push_str("      </RunSynchronous>\n");
    s.push_str("    </component>\n  </settings>\n");
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
    fn bypass_flags_emit_labconfig_writes() {
        let setup = WindowsSetup {
            bypass_tpm: true,
            bypass_ram: true,
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("pass=\"windowsPE\""));
        assert!(xml.contains("BypassTPMCheck"));
        assert!(xml.contains("BypassRAMCheck"));
        assert!(!xml.contains("BypassSecureBootCheck"));
    }

    #[test]
    fn local_account_and_msaccount_skip() {
        let setup = WindowsSetup {
            skip_msaccount: true,
            local_account: Some("Tom".into()),
            ..WindowsSetup::default()
        };
        let xml = generate(&setup);
        assert!(xml.contains("BypassNRO"));
        assert!(xml.contains("<Name>Tom</Name>"));
    }
}
