//! `windowsPE` pass — settings consumed before Windows is installed: the
//! Win 11 LabConfig hardware-check bypasses, the product key and EULA
//! accept, and the Setup-UI / system locale used by the installer itself.

use usbooty_core::WindowsSetup;

use super::{escape, push_component_per_arch, push_run_command};

pub(super) fn push_windows_pe(s: &mut String, setup: &WindowsSetup) {
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
