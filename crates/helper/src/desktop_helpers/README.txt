USBooty - Post-install desktop helpers
======================================

These scripts were dropped onto your Desktop by USBooty during the Windows
install. Each one is a small Windows batch (.bat) wrapper around a public,
well-known Windows cleanup, tweak, install or activation tool. They are
grouped into folders by purpose.

To run a script:
  - Right-click the .bat file
  - Choose "Run as administrator"

(Most of these need admin to actually change anything; the per-user ones say
so below.)


1 Debloat and Privacy
-------------------

Win11Debloat.bat
    Raphire's Win11Debloat. Downloads and runs the script published at
    https://debloat.raphi.re/ (source: github.com/Raphire/Win11Debloat).
    Interactive: lets you pick which Windows "features" to strip out
    (Cortana, Copilot, Recall, OneDrive, taskbar widgets, ads, etc.).

ChrisTitus-Winutil.bat
    Chris Titus Tech Windows Utility - STABLE channel.
    Downloads and runs the script published at https://christitus.com/win
    (source: github.com/ChrisTitusTech/winutil). A GUI for tweaks,
    debloat, install of common apps, Windows update controls.

ChrisTitus-Winutil-Dev.bat
    Same tool as above but the DEV channel (https://christitus.com/windev) -
    bleeding-edge build, newer features, occasional rough edges.

Remove-OneDrive.bat
    Stops OneDrive and runs both the 64-bit and 32-bit (WoW64)
    OneDriveSetup uninstallers. Bypasses Microsoft's "you can't remove
    OneDrive from Settings" UI block.

Remove-Windows-AI.bat
    Strips Copilot, Recall, generative-AI Paint / Photos / Notepad,
    AI-powered Search and Cortana hooks, and related AI telemetry
    components from Windows 11 using zoicware/RemoveWindowsAI
    (github.com/zoicware/RemoveWindowsAI). Run as administrator.

Winhance.bat
    GUI tool for debloating, optimising, customising and securing
    Windows 10 / 11 - removes bloat apps, tweaks privacy / telemetry
    settings, manages Microsoft Edge, and bundles common power-user
    optimisations. Source: github.com/memstechtips/Winhance
    (get.winhance.net). Run as administrator.


2 Tweaks and Performance
----------------------

FR33THY-Ultimate.bat
    FR33THY's Ultimate Windows Optimization (source:
    github.com/FR33THYFR33THY/Ultimate). Gaming / latency-focused
    tweaks: power plan, CPU scheduler, network stack, GPU settings,
    services trim, and a long list of registry tweaks aimed at
    minimising input latency and frame-time spikes. AGGRESSIVE - read
    the upstream README before running. Admin required.

Disable-FastStartup.bat
    Clears the Windows Fast Startup flag (HiberbootEnabled=0). Fast
    Startup hibernates the kernel on shutdown, which leaves the NTFS
    journal dirty and makes the partition mount read-only (or risks
    corruption) on Linux dual-boot. Hibernate itself stays available.
    Admin required.

Enable-LongPaths.bat
    Sets LongPathsEnabled=1 so Win32 paths can exceed the historic
    260-character MAX_PATH limit. Modern Git, Node.js, Python, Rust,
    .NET 6+ and PowerShell all opt in. A handful of older line-of-
    business apps may misbehave. Admin required; reboot recommended.

Disable-GameBar-GameDVR.bat
    Turns off the Xbox Game Bar and Game DVR background recording
    (GameDVR_Enabled / AppCaptureEnabled / AllowGameDVR = 0). Game DVR
    can add input latency and frame-time spikes. Admin required.

Enable-GPU-Scheduling.bat
    Enables Hardware-accelerated GPU Scheduling (HwSchMode=2) so the GPU
    manages its own VRAM scheduling, which can reduce latency and CPU
    overhead on supported GPUs. Admin required; reboot needed to apply.

Enable-Ultimate-Performance.bat
    Unlocks and activates Windows' hidden "Ultimate Performance" power
    plan (powercfg duplicatescheme + setactive). Best on desktops /
    plugged-in machines; raises power draw on laptops. Admin required.

Disable-Hibernation.bat
    Runs `powercfg -h off`: disables hibernation and deletes hiberfil.sys,
    freeing disk space worth a large fraction of RAM. Also turns off Fast
    Startup. Use Disable-FastStartup.bat instead if you want to keep
    Hibernate available. Admin required.

Enable-GodMode.bat
    Creates the "God Mode" (All Tasks) folder on your Desktop, a single
    view of every classic Control Panel / settings task. Per-user; no
    admin needed.

Restore-Classic-ContextMenu.bat
    Restores the full classic (Windows 10) right-click menu on Windows 11
    (no more "Show more options"), via an HKCU CLSID key, then restarts
    Explorer. Per-user; no admin needed. The script prints how to revert.


3 Install Apps
--------------

OfficeTool.bat
    Self-elevates, downloads OfficeTool Plus (the "with-runtime" bundle
    from https://otp.landian.vip/), extracts it to an "OfficeTool"
    folder on your Desktop, unblocks every file, and opens that folder
    in Explorer so you can launch "Office Tool Plus.exe" yourself.
    The .zip is removed from %TEMP% afterwards. OfficeTool Plus is a
    GUI front-end to the Microsoft Office Deployment Tool (ODT) for
    installing / configuring Microsoft Office without using the
    Microsoft Store.

Install-PowerToys.bat
    Installs Microsoft PowerToys via winget
    (source: github.com/microsoft/PowerToys). FancyZones (window
    snapping), PowerRename (batch rename), Color Picker, PowerToys Run
    launcher, Always-On-Top, Keyboard Manager, Mouse Highlighter, and
    more. Needs winget; run Install-Winget.bat first if missing.

Install-VCRedist.bat
    Installs the Microsoft Visual C++ Redistributable 2015-2022
    (unified package), both x64 and x86, via winget. Required by a
    large fraction of third-party software and games. Needs winget.

Install-DirectX.bat
    Installs the DirectX legacy runtime (D3DX, D3DCompiler, XAudio2)
    via winget (Microsoft.DirectX). Windows 10/11 already ship modern
    DirectX 11/12; this package covers the legacy libraries that
    many older games still link against. Needs winget.

Install-Browser.bat
    Interactive menu to install one of: Chrome (direct Google
    installer), Firefox, Brave, Zen, LibreWolf, Floorp, Waterfox,
    Opera, Opera GX, Vivaldi, Arc. Every option except Chrome uses
    winget; the Chrome path downloads the official installer from
    dl.google.com and runs it /silent /install.

Install-DotNet-Runtimes.bat
    Installs the .NET Desktop Runtime (WPF / WinForms), both the LTS
    (8) and current (9) x64 builds, via winget. The .NET-era complement
    to the Visual C++ Redistributable; many modern desktop apps need it.
    Needs winget.


4 Package Managers
------------------

Install-Chocolatey.bat
    Installs the Chocolatey package manager (source:
    community.chocolatey.org/install). System-wide; needs admin. After
    install you can run e.g. `choco install vlc 7zip git -y`.

Install-Scoop.bat
    Installs the Scoop command-line installer (source: get.scoop.sh).
    Per-user: do NOT run this one as administrator. After install
    you can run e.g. `scoop install git neovim ripgrep`.

Install-Winget.bat
    Installs or repairs winget (Windows Package Manager) using the
    asheroto/winget-install script
    (github.com/asheroto/winget-install). Recent Windows 10 / 11 builds
    already ship winget; this script is for builds (incl. Server SKUs)
    that don't have it or where it's broken.


5 Activation
------------

Massgravel-Activator.bat
    Microsoft Activation Scripts (MAS) by Massgrave. Downloads and runs
    https://get.activated.win (source:
    github.com/massgravel/Microsoft-Activation-Scripts).
    Activates Windows / Office via documented HWID, KMS38, and Online
    KMS methods.


The debloat suites, activator, package managers and app installers fetch code
over the public internet and run it; the tweak scripts (Fast Startup, Long
Paths, Game DVR, GPU scheduling, Ultimate Performance, Hibernation, God Mode,
classic context menu) only change local registry / power settings. Open any
.bat in Notepad first if you want to see exactly what it does.
