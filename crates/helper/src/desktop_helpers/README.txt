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
    Downloads the OfficeTool runtime ZIP from https://otp.landian.vip/
    into your Downloads folder. OfficeTool is a GUI front-end to the
    Microsoft Office Deployment Tool (ODT) for installing / configuring
    Microsoft Office without using the Microsoft Store.

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

Each of these scripts fetches code over the public internet and runs it.
Open the .bat in Notepad first if you want to see the exact URL it hits.
