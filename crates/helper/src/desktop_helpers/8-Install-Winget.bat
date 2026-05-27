@echo off
title Install / repair winget

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
echo Windows Package Manager (winget) installer by asheroto
echo Source: https://github.com/asheroto/winget-install
echo.
echo Recent Windows 10 / 11 builds ship with winget pre-installed; this
echo script installs or repairs it on builds that don't, and on Server
echo SKUs where winget is missing by default.
echo.
echo Run as administrator for a machine-wide install.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://github.com/asheroto/winget-install/releases/latest/download/winget-install.ps1 | iex"
echo.
echo If install succeeded, open a new terminal and try: winget --version
echo.
pause
