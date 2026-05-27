//! Data assets embedded into the helper at build time: the debloat policy,
//! the desktop-helpers bundle, the supported processor-architecture list,
//! and the PowerShell one-liners run from `RunSynchronousCommand` entries.
//!
//! Kept here, separately from the XML-building logic in [`super`], so adding
//! or replacing a vendored asset is a one-file change and the unattend
//! generator stays focused on schema mechanics.

/// The vendored debloat policy, written next to `autounattend.xml` when
/// [`usbooty_core::WindowsSetup::apply_debloat`] is set.
pub(super) const DEBLOAT_REG: &str = include_str!("../debloat.reg");

/// The filename of the debloat policy on the USB root. The autounattend's
/// `specialize`-pass `for` loop scans D..Z for this exact name.
pub(super) const DEBLOAT_REG_NAME: &str = "usbooty-debloat.reg";

/// Name of the folder on the USB that holds the post-install `.bat`
/// helpers. The `specialize`-pass `for` loop scans drive letters for
/// `<letter>:\<DESKTOP_HELPERS_DIR>\<DESKTOP_HELPERS_SENTINEL>` to find
/// the install media and xcopies the whole folder to the Default user's
/// Desktop so every new account inherits it.
pub(super) const DESKTOP_HELPERS_DIR: &str = "USBooty";
pub(super) const DESKTOP_HELPERS_SENTINEL: &str = "1-Win11Debloat.bat";

/// Each entry is `(filename, contents)`. The files are written verbatim
/// into `<mount>/USBooty/` when [`usbooty_core::WindowsSetup::desktop_helpers`]
/// is set.
pub(super) const DESKTOP_HELPERS: &[(&str, &str)] = &[
    (
        "1-Win11Debloat.bat",
        include_str!("../desktop_helpers/1-Win11Debloat.bat"),
    ),
    (
        "2-ChrisTitus-Winutil.bat",
        include_str!("../desktop_helpers/2-ChrisTitus-Winutil.bat"),
    ),
    (
        "2.1-ChrisTitus-Winutil-Dev.bat",
        include_str!("../desktop_helpers/2.1-ChrisTitus-Winutil-Dev.bat"),
    ),
    (
        "3-Massgravel-Activator.bat",
        include_str!("../desktop_helpers/3-Massgravel-Activator.bat"),
    ),
    (
        "4-Remove-OneDrive.bat",
        include_str!("../desktop_helpers/4-Remove-OneDrive.bat"),
    ),
    (
        "5-OfficeTool.bat",
        include_str!("../desktop_helpers/5-OfficeTool.bat"),
    ),
    (
        "6-Install-Chocolatey.bat",
        include_str!("../desktop_helpers/6-Install-Chocolatey.bat"),
    ),
    (
        "7-Install-Scoop.bat",
        include_str!("../desktop_helpers/7-Install-Scoop.bat"),
    ),
    (
        "8-Install-Winget.bat",
        include_str!("../desktop_helpers/8-Install-Winget.bat"),
    ),
    (
        "9-Remove-Windows-AI.bat",
        include_str!("../desktop_helpers/9-Remove-Windows-AI.bat"),
    ),
    (
        "10-Winhance.bat",
        include_str!("../desktop_helpers/10-Winhance.bat"),
    ),
    (
        "11-FR33THY-Ultimate.bat",
        include_str!("../desktop_helpers/11-FR33THY-Ultimate.bat"),
    ),
    ("README.txt", include_str!("../desktop_helpers/README.txt")),
];

/// Supported processor architectures. Windows Setup matches `<component>` by
/// architecture, so emitting only one would silently skip ARM and x86 hosts.
pub(super) const ARCHITECTURES: [&str; 3] = ["x86", "arm64", "amd64"];

/// PowerShell one-liner that scans drive letters C..K for the install media's
/// `sources\sxs` folder and enables the .NET Framework 3.5 component from
/// there. The first match wins; missing drives no-op.
pub(super) const DOTNET35_COMMAND: &str = concat!(
    r#"powershell.exe -NoProfile -WindowStyle Hidden -Command ""#,
    r#"foreach($d in 'C','D','E','F','G','H','I','J','K'){"#,
    r#"$src=Join-Path ($d+':') 'sources\sxs';"#,
    r#"if(Test-Path $src\*.cab){"#,
    r#"dism /Online /Enable-Feature /FeatureName:NetFx3 /All /LimitAccess /Source:$src;"#,
    r#"break}}""#,
);

/// PowerShell command that disables every network adapter — used in the
/// `specialize` pass to force OOBE down the local-account path on Win 11 24H2+.
pub(super) const DISABLE_ADAPTERS_COMMAND: &str = concat!(
    r#"powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass "#,
    r#"-Command "Get-NetAdapter | Disable-NetAdapter -Confirm:$false""#,
);

/// PowerShell command that re-enables every network adapter on first logon.
pub(super) const ENABLE_ADAPTERS_COMMAND: &str = concat!(
    r#"powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass "#,
    r#"-Command "Get-NetAdapter | Enable-NetAdapter -Confirm:$false""#,
);
