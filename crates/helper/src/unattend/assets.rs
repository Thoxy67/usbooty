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
/// helpers (grouped into category subfolders, see [`DESKTOP_HELPERS`]). The
/// `specialize`-pass `for` loop scans drive letters for
/// `<letter>:\<DESKTOP_HELPERS_DIR>\<DESKTOP_HELPERS_SENTINEL>` to find the
/// install media and xcopies the whole tree (`xcopy /E`, recursing the
/// subfolders) to the Default user's Desktop so every new account inherits it.
pub(super) const DESKTOP_HELPERS_DIR: &str = "USBooty";
/// The marker the specialize `for` loop checks to confirm it found the USBooty
/// folder on the right drive. Kept at the folder *root* (not inside a category
/// subfolder) so the `if exist` path has no spaces or `&` to trip up `cmd`.
pub(super) const DESKTOP_HELPERS_SENTINEL: &str = "README.txt";

/// Each entry is `(relative_path, contents)`. The path is written verbatim
/// under `<mount>/USBooty/` (creating the category subfolder) when
/// [`usbooty_core::WindowsSetup::desktop_helpers`] is set. The category
/// subfolders carry a leading number so they sort in a sensible order in
/// Explorer; the scripts themselves are unnumbered.
pub(super) const DESKTOP_HELPERS: &[(&str, &str)] = &[
    // --- 1 Debloat & Privacy ---
    (
        "1 Debloat & Privacy/Win11Debloat.bat",
        include_str!("../desktop_helpers/1 Debloat & Privacy/Win11Debloat.bat"),
    ),
    (
        "1 Debloat & Privacy/ChrisTitus-Winutil.bat",
        include_str!("../desktop_helpers/1 Debloat & Privacy/ChrisTitus-Winutil.bat"),
    ),
    (
        "1 Debloat & Privacy/ChrisTitus-Winutil-Dev.bat",
        include_str!("../desktop_helpers/1 Debloat & Privacy/ChrisTitus-Winutil-Dev.bat"),
    ),
    (
        "1 Debloat & Privacy/Remove-OneDrive.bat",
        include_str!("../desktop_helpers/1 Debloat & Privacy/Remove-OneDrive.bat"),
    ),
    (
        "1 Debloat & Privacy/Remove-Windows-AI.bat",
        include_str!("../desktop_helpers/1 Debloat & Privacy/Remove-Windows-AI.bat"),
    ),
    (
        "1 Debloat & Privacy/Winhance.bat",
        include_str!("../desktop_helpers/1 Debloat & Privacy/Winhance.bat"),
    ),
    // --- 2 Tweaks & Performance ---
    (
        "2 Tweaks & Performance/FR33THY-Ultimate.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/FR33THY-Ultimate.bat"),
    ),
    (
        "2 Tweaks & Performance/Disable-FastStartup.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Disable-FastStartup.bat"),
    ),
    (
        "2 Tweaks & Performance/Enable-LongPaths.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Enable-LongPaths.bat"),
    ),
    (
        "2 Tweaks & Performance/Disable-GameBar-GameDVR.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Disable-GameBar-GameDVR.bat"),
    ),
    (
        "2 Tweaks & Performance/Enable-GPU-Scheduling.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Enable-GPU-Scheduling.bat"),
    ),
    (
        "2 Tweaks & Performance/Enable-Ultimate-Performance.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Enable-Ultimate-Performance.bat"),
    ),
    (
        "2 Tweaks & Performance/Disable-Hibernation.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Disable-Hibernation.bat"),
    ),
    (
        "2 Tweaks & Performance/Enable-GodMode.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Enable-GodMode.bat"),
    ),
    (
        "2 Tweaks & Performance/Restore-Classic-ContextMenu.bat",
        include_str!("../desktop_helpers/2 Tweaks & Performance/Restore-Classic-ContextMenu.bat"),
    ),
    // --- 3 Install Apps ---
    (
        "3 Install Apps/OfficeTool.bat",
        include_str!("../desktop_helpers/3 Install Apps/OfficeTool.bat"),
    ),
    (
        "3 Install Apps/Install-PowerToys.bat",
        include_str!("../desktop_helpers/3 Install Apps/Install-PowerToys.bat"),
    ),
    (
        "3 Install Apps/Install-VCRedist.bat",
        include_str!("../desktop_helpers/3 Install Apps/Install-VCRedist.bat"),
    ),
    (
        "3 Install Apps/Install-DirectX.bat",
        include_str!("../desktop_helpers/3 Install Apps/Install-DirectX.bat"),
    ),
    (
        "3 Install Apps/Install-Browser.bat",
        include_str!("../desktop_helpers/3 Install Apps/Install-Browser.bat"),
    ),
    (
        "3 Install Apps/Install-DotNet-Runtimes.bat",
        include_str!("../desktop_helpers/3 Install Apps/Install-DotNet-Runtimes.bat"),
    ),
    // --- 4 Package Managers ---
    (
        "4 Package Managers/Install-Chocolatey.bat",
        include_str!("../desktop_helpers/4 Package Managers/Install-Chocolatey.bat"),
    ),
    (
        "4 Package Managers/Install-Scoop.bat",
        include_str!("../desktop_helpers/4 Package Managers/Install-Scoop.bat"),
    ),
    (
        "4 Package Managers/Install-Winget.bat",
        include_str!("../desktop_helpers/4 Package Managers/Install-Winget.bat"),
    ),
    // --- 5 Activation ---
    (
        "5 Activation/Massgravel-Activator.bat",
        include_str!("../desktop_helpers/5 Activation/Massgravel-Activator.bat"),
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

/// PowerShell command that disables every network adapter, used in the
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

/// `sources/ei.cfg` payload, written when
/// [`usbooty_core::WindowsSetup::force_edition_picker`] is set.
///
/// The mere presence of the file disables Windows Setup's auto-use of the
/// firmware MSDM/SLIC OEM key (the mechanism that silently installs Home
/// on a Family-licensed OEM machine), so the user lands on Setup's
/// built-in edition picker after the "I don't have a product key" link
/// on the activation prompt.
///
/// `Channel = _Default` is the documented "let Setup decide" value
/// (versus `Retail` / `OEM` / `Volume` which would assert one of those
/// channels). `VL = 0` keeps Setup off the Volume-Licensing code path.
/// Omitting `[EditionID]` entirely is sufficient to defer the choice to
/// the picker, so we don't write a blank section for it.
pub(super) const EI_CFG: &str = "[Channel]\n_Default\n[VL]\n0\n";

/// Filename for the file written from [`EI_CFG`], under the USB's `sources/`
/// directory at the root of the install media.
pub(super) const EI_CFG_NAME: &str = "ei.cfg";

/// Subdirectory on the USB where Windows Setup looks for `ei.cfg`. Resolved
/// case-insensitively against the directory the ISO actually copied (lower-cased
/// `sources` on real media), so it never creates a second, case-colliding
/// directory on a case-sensitive destination. See [`crate::fsutil::ci_join`].
pub(super) const EI_CFG_DIR: &str = "sources";
