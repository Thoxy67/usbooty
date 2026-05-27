@echo off
title Install Microsoft PowerToys

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
echo Microsoft PowerToys
echo Source: https://github.com/microsoft/PowerToys
echo.
echo The official Microsoft utility suite: FancyZones (window snapping),
echo PowerRename (batch rename), Color Picker, PowerToys Run launcher,
echo Always-On-Top, Keyboard Manager, Mouse Highlighter, and more.
echo.
echo This uses winget. If winget is missing, run 8-Install-Winget.bat
echo first and then come back to this one.
echo.
winget install --exact --id Microsoft.PowerToys --accept-source-agreements --accept-package-agreements
echo.
echo Launch PowerToys from the Start menu after install to enable the
echo individual modules you want.
echo.
pause
