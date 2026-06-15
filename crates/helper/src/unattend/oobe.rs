//! `oobeSystem` pass, first-boot user-facing settings: which OOBE prompts
//! to hide, the auto-logon + local account creation, the FirstLogonCommands
//! that undo specialize-time network blocks, and the user-facing locale.

use usbooty_core::WindowsSetup;

use super::assets::{ENABLE_ADAPTERS_COMMAND, PASSWORD_NEVER_EXPIRES_COMMAND};
use super::{
    Locale, escape, parse_locale, push_component_per_arch, push_first_logon_commands, target_archs,
};

pub(super) fn push_oobe_system(s: &mut String, setup: &WindowsSetup) {
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
        || setup.disable_network_during_oobe
        || setup.prevent_password_expiration;
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
    if setup.prevent_password_expiration {
        first_logon.push((
            "Set local accounts' password to never expire",
            PASSWORD_NEVER_EXPIRES_COMMAND,
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
        shell_body.push_str("            <Group>Administrators;Power Users</Group>\n");
        shell_body.push_str("          </LocalAccount>\n");
        shell_body.push_str("        </LocalAccounts>\n      </UserAccounts>\n");
    }

    let mut intl_body = String::new();
    if let Some(Locale {
        primary,
        input_locale,
    }) = oobe_locale.and_then(parse_locale)
    {
        intl_body.push_str(&format!(
            "      <InputLocale>{input_locale}</InputLocale>\n"
        ));
        intl_body.push_str(&format!("      <SystemLocale>{primary}</SystemLocale>\n"));
        intl_body.push_str(&format!("      <UILanguage>{primary}</UILanguage>\n"));
        intl_body.push_str(&format!("      <UserLocale>{primary}</UserLocale>\n"));
    }

    let archs = target_archs(setup);
    s.push_str("  <settings pass=\"oobeSystem\">\n");
    if !shell_body.is_empty() {
        push_component_per_arch(s, &archs, "Microsoft-Windows-Shell-Setup", &shell_body);
    }
    if !intl_body.is_empty() {
        push_component_per_arch(
            s,
            &archs,
            "Microsoft-Windows-International-Core",
            &intl_body,
        );
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
