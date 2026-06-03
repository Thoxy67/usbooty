@echo off
title Install ExplorerPatcher (latest)

REM Self-elevate if not already running as administrator. `fltmc` is a
REM lightweight privileged-only probe: a non-zero exit means we are
REM running unelevated, so relaunch via PowerShell's RunAs verb (which
REM triggers the UAC prompt) and let the original low-priv shell exit.
>nul 2>&1 fltmc
if not "%errorlevel%"=="0" (
    echo Requesting administrator privileges ...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)
echo ExplorerPatcher - latest release from github.com/valinet/ExplorerPatcher
echo.
echo Brings back the Windows 10 taskbar, Start menu and File Explorer
echo behaviour on Windows 11. This grabs the newest signed installer that
echo matches your CPU architecture (x64 or ARM64) straight from the
echo project's GitHub releases, then launches it elevated.
echo.
echo It is a deep Explorer customization tool: review the project page if
echo you are unsure. You can remove it later from "Apps and features" or
echo by running ep_setup.exe again.
echo.
echo Querying the latest release and downloading ...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; if([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64'){$want='ep_setup_arm64.exe'}else{$want='ep_setup.exe'}; $rel=Invoke-RestMethod -UseBasicParsing -Headers @{'User-Agent'='usbooty'} -Uri 'https://api.github.com/repos/valinet/ExplorerPatcher/releases/latest'; $asset=$rel.assets | Where-Object { $_.name -eq $want } | Select-Object -First 1; if(-not $asset){throw ('No asset named ' + $want + ' in release ' + $rel.tag_name)}; $out=Join-Path $env:TEMP $want; Write-Host ('Downloading ' + $want + ' (release ' + $rel.tag_name + ') ...'); Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $out; Write-Host 'Launching the installer ...'; Start-Process -FilePath $out -Wait"
if not "%errorlevel%"=="0" (
    echo.
    echo Something went wrong while downloading or launching the installer.
    echo Check your internet connection and try again.
)
echo.
echo Done. If the taskbar did not change yet, sign out and back in.
echo.
pause
