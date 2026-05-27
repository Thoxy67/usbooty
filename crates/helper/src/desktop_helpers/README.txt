USBooty - Post-install desktop helpers
======================================

These scripts were dropped onto your Desktop by USBooty during the Windows
install. Each one is a small Windows batch (.bat) wrapper around a public,
well-known Windows cleanup or activation tool.

To run a script:
  - Right-click the .bat file
  - Choose "Run as administrator"

(Most of these need admin to actually change anything.)

Scripts
-------

1-Win11Debloat.bat
    Raphire's Win11Debloat. Downloads and runs the script published at
    https://debloat.raphi.re/ (source: github.com/Raphire/Win11Debloat).
    Interactive: lets you pick which Windows "features" to strip out
    (Cortana, Copilot, Recall, OneDrive, taskbar widgets, ads, etc.).

2-ChrisTitus-Winutil.bat
    Chris Titus Tech Windows Utility - STABLE channel.
    Downloads and runs the script published at https://christitus.com/win
    (source: github.com/ChrisTitusTech/winutil). A GUI for tweaks,
    debloat, install of common apps, Windows update controls.

2.1-ChrisTitus-Winutil-Dev.bat
    Same tool as #2 but the DEV channel (https://christitus.com/windev) -
    bleeding-edge build, newer features, occasional rough edges.

3-Massgravel-Activator.bat
    Microsoft Activation Scripts (MAS) by Massgrave. Downloads and runs
    https://get.activated.win (source:
    github.com/massgravel/Microsoft-Activation-Scripts).
    Activates Windows / Office via documented HWID, KMS38, and Online
    KMS methods.

4-Remove-OneDrive.bat
    Stops OneDrive and runs both the 64-bit and 32-bit (WoW64)
    OneDriveSetup uninstallers. Bypasses Microsoft's "you can't remove
    OneDrive from Settings" UI block.

5-OfficeTool.bat
    Self-elevates, downloads OfficeTool Plus (the "with-runtime" bundle
    from https://otp.landian.vip/), extracts it to an "OfficeTool"
    folder on your Desktop, unblocks every file, and opens that folder
    in Explorer so you can launch "Office Tool Plus.exe" yourself.
    The .zip is removed from %TEMP% afterwards. OfficeTool Plus is a
    GUI front-end to the Microsoft Office Deployment Tool (ODT) for
    installing / configuring Microsoft Office without using the
    Microsoft Store.

6-Install-Chocolatey.bat
    Installs the Chocolatey package manager (source:
    community.chocolatey.org/install). System-wide; needs admin. After
    install you can run e.g. `choco install vlc 7zip git -y`.

7-Install-Scoop.bat
    Installs the Scoop command-line installer (source: get.scoop.sh).
    Per-user — do NOT run this one as administrator. After install
    you can run e.g. `scoop install git neovim ripgrep`.

8-Install-Winget.bat
    Installs or repairs winget (Windows Package Manager) using the
    asheroto/winget-install script
    (github.com/asheroto/winget-install). Recent Windows 10 / 11 builds
    already ship winget; this script is for builds (incl. Server SKUs)
    that don't have it or where it's broken.

9-Remove-Windows-AI.bat
    Strips Copilot, Recall, generative-AI Paint / Photos / Notepad,
    AI-powered Search and Cortana hooks, and related AI telemetry
    components from Windows 11 using zoicware/RemoveWindowsAI
    (github.com/zoicware/RemoveWindowsAI). Run as administrator.

10-Winhance.bat
    GUI tool for debloating, optimising, customising and securing
    Windows 10 / 11 - removes bloat apps, tweaks privacy / telemetry
    settings, manages Microsoft Edge, and bundles common power-user
    optimisations. Source: github.com/memstechtips/Winhance
    (get.winhance.net). Run as administrator.

11-FR33THY-Ultimate.bat
    FR33THY's Ultimate Windows Optimization (source:
    github.com/FR33THYFR33THY/Ultimate). Gaming / latency-focused
    tweaks: power plan, CPU scheduler, network stack, GPU settings,
    services trim, and a long list of registry tweaks aimed at
    minimising input latency and frame-time spikes. AGGRESSIVE - read
    the upstream README before running. Admin required.

12-Install-PowerToys.bat
    Installs Microsoft PowerToys via winget
    (source: github.com/microsoft/PowerToys). FancyZones (window
    snapping), PowerRename (batch rename), Color Picker, PowerToys Run
    launcher, Always-On-Top, Keyboard Manager, Mouse Highlighter, and
    more. Needs winget — run 8-Install-Winget.bat first if missing.

13-Disable-FastStartup.bat
    Clears the Windows Fast Startup flag (HiberbootEnabled=0). Fast
    Startup hibernates the kernel on shutdown, which leaves the NTFS
    journal dirty and makes the partition mount read-only (or risks
    corruption) on Linux dual-boot. Hibernate itself stays available.
    Admin required.

14-Enable-LongPaths.bat
    Sets LongPathsEnabled=1 so Win32 paths can exceed the historic
    260-character MAX_PATH limit. Modern Git, Node.js, Python, Rust,
    .NET 6+ and PowerShell all opt in. A handful of older line-of-
    business apps may misbehave. Admin required; reboot recommended.

15-Install-VCRedist.bat
    Installs the Microsoft Visual C++ Redistributable 2015-2022
    (unified package), both x64 and x86, via winget. Required by a
    large fraction of third-party software and games. Needs winget.

16-Install-DirectX.bat
    Installs the DirectX legacy runtime (D3DX, D3DCompiler, XAudio2)
    via winget (Microsoft.DirectX). Windows 10/11 already ship modern
    DirectX 11/12; this package covers the legacy libraries that
    many older games still link against. Needs winget.

17-Install-Browser.bat
    Interactive menu to install one of: Chrome (direct Google
    installer), Firefox, Brave, Zen, LibreWolf, Floorp, Waterfox,
    Opera, Opera GX, Vivaldi, Arc. Every option except Chrome uses
    winget; the Chrome path downloads the official installer from
    dl.google.com and runs it /silent /install.

Each of these scripts fetches code over the public internet and runs it.
Open the .bat in Notepad first if you want to see the exact URL it hits.
