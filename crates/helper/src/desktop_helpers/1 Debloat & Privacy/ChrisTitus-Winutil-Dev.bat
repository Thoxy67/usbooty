@echo off
title Chris Titus Tech - Windows Utility (dev)

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
echo Chris Titus Tech Windows Utility (dev channel)
echo Source: https://github.com/ChrisTitusTech/winutil
echo.
echo Fetching and running the script from https://christitus.com/windev ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm 'https://christitus.com/windev' | iex"
echo.
pause
