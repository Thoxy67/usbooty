@echo off
title Microsoft Activation Scripts (Massgrave)

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
echo Microsoft Activation Scripts (MAS) by Massgrave
echo Source: https://github.com/massgravel/Microsoft-Activation-Scripts
echo.
echo Fetching and running the script from https://get.activated.win ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm 'https://get.activated.win' | iex"
echo.
pause
