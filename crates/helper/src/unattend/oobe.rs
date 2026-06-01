//! `oobeSystem` pass, first-boot user-facing settings: which OOBE prompts
//! to hide, the auto-logon + local account creation, the FirstLogonCommands
//! that undo specialize-time network blocks, and the user-facing locale.

use usbooty_core::WindowsSetup;

use super::assets::ENABLE_ADAPTERS_COMMAND;
use super::{escape, push_component_per_arch, push_first_logon_commands};

/// The Base64 (UTF-16LE) blob Rufus uses for a Windows To Go local account's
/// password. It decodes to the literal string "Password" but, per Microsoft's
/// convention, initializes the account with an **empty** password (the user is
/// then forced to set one at first logon). Copied verbatim from Rufus
/// (`wue.c`, `UABhAHMAcwB3AG8AcgBkAA==`).
const WTG_EMPTY_PASSWORD_B64: &str = "UABhAHMAcwB3AG8AcgBkAA==";

/// Account names Windows reserves (built-in / well-known). A local account with
/// one of these names is rejected, matching Rufus's `unallowed_account_names`.
fn is_reserved_account_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "administrator",
        "administrateur",
        "administrador",
        "guest",
        "defaultaccount",
        "wdagutilityaccount",
        "helpassistant",
        "krbtgt",
        "local",
        "none",
        "system",
    ];
    let lower = name.to_ascii_lowercase();
    RESERVED.contains(&lower.as_str())
}

/// Sanitize a local-account name the way Rufus does: replace the characters
/// Windows forbids in a `LocalAccount` `Name` (`/\[]:|<>+=;,?*%@.`) with `_`.
fn sanitize_account_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if "/\\[]:|<>+=;,?*%@.".contains(c) { '_' } else { c })
        .collect()
}

/// Emit the Windows To Go `oobeSystem` pass, faithful to Rufus's default WTG
/// answer file. Unlike the install path ([`push_oobe_system`]) there is **never**
/// an `<AutoLogon>`: when a local account is requested it is pre-seeded with an
/// empty password (`PlainText=false`, the [`WTG_EMPTY_PASSWORD_B64`] blob),
/// placed in `Administrators;Power Users`, and forced to change its password at
/// first logon. The interactive OOBE first-sign-in still runs, so the modern
/// 24H2/25H2 OOBE controller accepts setup as complete and does not reseal-loop.
/// With no account requested the pass carries only the OOBE hide flags / locale,
/// and the user creates their account interactively.
pub(super) fn push_oobe_system_wtg(s: &mut String, setup: &WindowsSetup) {
    let oobe_items = build_oobe_items(setup);
    let account = setup
        .local_account
        .as_deref()
        .map(sanitize_account_name)
        .filter(|n| !n.is_empty() && !is_reserved_account_name(n));
    let oobe_locale = setup.locale.as_deref().filter(|l| !l.is_empty());

    if oobe_items.is_empty() && account.is_none() && oobe_locale.is_none() {
        return;
    }

    let mut shell_body = String::new();
    // Shell-Setup schema order: FirstLogonCommands → OOBE → UserAccounts.
    // (No AutoLogon for WTG, by design.)
    let logonpasswordchg;
    let mut first_logon: Vec<(&str, &str)> = Vec::new();
    if let Some(name) = &account {
        // Force the empty-password account to set a real password at first
        // logon, then stop the `net user` change from also capping password age.
        logonpasswordchg = format!("net user \"{}\" /logonpasswordchg:yes", name);
        first_logon.push(("Require a password change at first logon", &logonpasswordchg));
        first_logon.push((
            "Keep local-account passwords from expiring",
            "net accounts /maxpwage:unlimited",
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
    if let Some(name) = &account {
        shell_body.push_str("      <UserAccounts>\n        <LocalAccounts>\n");
        shell_body.push_str("          <LocalAccount wcm:action=\"add\">\n");
        shell_body.push_str("            <Password>\n");
        shell_body.push_str(&format!("              <Value>{WTG_EMPTY_PASSWORD_B64}</Value>\n"));
        shell_body.push_str("              <PlainText>false</PlainText>\n");
        shell_body.push_str("            </Password>\n");
        shell_body.push_str(&format!("            <Name>{}</Name>\n", escape(name)));
        shell_body.push_str(&format!("            <DisplayName>{}</DisplayName>\n", escape(name)));
        shell_body.push_str("            <Group>Administrators;Power Users</Group>\n");
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
