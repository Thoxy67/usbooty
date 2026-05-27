@echo off
title Remove OneDrive

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
echo Killing the OneDrive process if running...
taskkill /f /im OneDrive.exe 2>nul
echo.
echo Running the 64-bit OneDrive uninstaller...
"%SystemRoot%\System32\OneDriveSetup.exe" /uninstall
echo.
echo Running the 32-bit (WoW64) OneDrive uninstaller...
"%SystemRoot%\SysWOW64\OneDriveSetup.exe" /uninstall
echo.
echo OneDrive removal finished. You may need to sign out / restart for the
echo OneDrive shell entry to fully disappear.
echo.
pause
