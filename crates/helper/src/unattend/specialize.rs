//! `specialize` pass, post-image, pre-OOBE machine settings: the BypassNRO
//! fallback, the no-network-during-OOBE trick, the .NET 3.5 enabler, the
//! computer name, the time zone, and the debloat-policy / desktop-helpers
//! imports staged onto the USB by [`super::write`].

use usbooty_core::WindowsSetup;

use super::assets::{
    DEBLOAT_REG_NAME, DESKTOP_HELPERS_DIR, DESKTOP_HELPERS_SENTINEL, DISABLE_ADAPTERS_COMMAND,
    DOTNET35_COMMAND,
};
use super::{
    escape, push_component_per_arch, push_run_command, sanitize_computer_name, target_archs,
};

pub(super) fn push_specialize(s: &mut String, setup: &WindowsSetup) {
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
    if setup.disable_fast_startup {
        // Clear HiberbootEnabled so a full shutdown really powers off (and
        // releases the disks) instead of hibernating the kernel, the usual
        // dual-boot fix. Machine-wide, so it sits in HKLM like the others.
        deploy_cmds.push((
            "reg add \"HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power\" \
             /v HiberbootEnabled /t REG_DWORD /d 0 /f"
                .to_string(),
            Some("Disable Fast Startup (hybrid shutdown)"),
        ));
    }
    // Per-user tweaks that live in the default user's NTUSER.DAT. They're
    // applied by mounting it as HKU\DFT so every account cloned from Default
    // (including the one created at OOBE) inherits them. The debloat .reg
    // import shares this same mount window, so we load/unload only once.
    let mut dft_cmds: Vec<(String, Option<&'static str>)> = Vec::new();
    if setup.show_file_extensions {
        dft_cmds.push((
            "reg add \"HKU\\DFT\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced\" \
             /v HideFileExt /t REG_DWORD /d 0 /f"
                .to_string(),
            Some("Show known file extensions in Explorer"),
        ));
    }
    if setup.show_hidden_files {
        dft_cmds.push((
            "reg add \"HKU\\DFT\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced\" \
             /v Hidden /t REG_DWORD /d 1 /f"
                .to_string(),
            Some("Show hidden files in Explorer"),
        ));
    }
    if setup.dark_mode {
        dft_cmds.push((
            "reg add \"HKU\\DFT\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\" \
             /v AppsUseLightTheme /t REG_DWORD /d 0 /f"
                .to_string(),
            Some("Use the dark theme for apps"),
        ));
        dft_cmds.push((
            "reg add \"HKU\\DFT\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\" \
             /v SystemUsesLightTheme /t REG_DWORD /d 0 /f"
                .to_string(),
            Some("Use the dark theme for the system UI"),
        ));
    }
    if setup.apply_debloat || !dft_cmds.is_empty() {
        deploy_cmds.push((
            "reg load HKU\\DFT C:\\Users\\Default\\NTUSER.DAT".to_string(),
            Some("Mount the default user's hive for per-user tweaks"),
        ));
        deploy_cmds.extend(dft_cmds);
        if setup.apply_debloat {
            // The USB drive letter is unpredictable on a freshly-installing
            // system, so scan D..Z for the .reg next to autounattend.xml.
            deploy_cmds.push((
                format!(
                    "cmd /c \"for %d in (D E F G H I J K L M N O P Q R S T U V W X Y Z) \
                     do if exist %d:\\{name} reg import %d:\\{name}\"",
                    name = DEBLOAT_REG_NAME,
                ),
                Some("Import usbooty-debloat.reg from the USB"),
            ));
        }
        deploy_cmds.push((
            "reg unload HKU\\DFT".to_string(),
            Some("Unmount the default user's hive"),
        ));
    }
    if setup.classic_context_menu {
        // Restore the Win 10 "show more options" menu by giving the Win 11
        // command-bar CLSID an empty InprocServer32 handler. This lives in
        // the *per-user class store* (UsrClass.dat), a different file from
        // NTUSER.DAT, so it gets its own mount. Empty value (/ve) is the
        // documented disable switch. Harmless on Windows 10.
        deploy_cmds.push((
            "reg load HKU\\DFTClasses \
             \"C:\\Users\\Default\\AppData\\Local\\Microsoft\\Windows\\UsrClass.dat\""
                .to_string(),
            Some("Mount the default user's class store"),
        ));
        deploy_cmds.push((
            "reg add \"HKU\\DFTClasses\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\
             \\InprocServer32\" /ve /f"
                .to_string(),
            Some("Restore the Windows 10 classic right-click menu"),
        ));
        deploy_cmds.push((
            "reg unload HKU\\DFTClasses".to_string(),
            Some("Unmount the default user's class store"),
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
        // The destination is deliberately unquoted: it contains no spaces,
        // and a quoted path ending in `\"` hits the classic Windows argv
        // rule where `\"` is an escaped quote, mangling the argument and
        // failing xcopy (which would fail Windows Setup's specialize pass).
        deploy_cmds.push((
            format!(
                "cmd /c \"for %d in (D E F G H I J K L M N O P Q R S T U V W X Y Z) \
                 do if exist %d:\\{dir}\\{sentinel} \
                 xcopy /E /I /Y /Q %d:\\{dir} C:\\Users\\Default\\Desktop\\{dir}\"",
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

    let archs = target_archs(setup);
    s.push_str("  <settings pass=\"specialize\">\n");
    if !deploy_body.is_empty() {
        push_component_per_arch(s, &archs, "Microsoft-Windows-Deployment", &deploy_body);
    }
    if !shell_body.is_empty() {
        push_component_per_arch(s, &archs, "Microsoft-Windows-Shell-Setup", &shell_body);
    }
    s.push_str("  </settings>\n");
}
