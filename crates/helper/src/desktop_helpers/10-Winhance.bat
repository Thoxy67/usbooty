@echo off
title Winhance

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
echo Winhance - Windows enhancement utility
echo Source: https://github.com/memstechtips/Winhance
echo.
echo GUI tool for debloating, optimising, customising and securing
echo Windows 10 / 11 (removes bloat apps, tweaks privacy / telemetry
echo settings, manages Microsoft Edge, etc.).
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm 'https://get.winhance.net' | iex"
echo.
pause
