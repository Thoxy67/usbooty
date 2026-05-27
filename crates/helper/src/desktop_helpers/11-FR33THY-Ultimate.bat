@echo off
title FR33THY Ultimate Windows Optimization

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
echo FR33THY's Ultimate Windows Optimization
echo Source: https://github.com/FR33THYFR33THY/Ultimate
echo.
echo Gaming / latency-focused tweaks: power plan, scheduler, network
echo stack, GPU driver settings, services trim, and a long list of
echo registry optimisations targeted at minimising input latency and
echo frame-time spikes.
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "iwr https://github.com/FR33THYFR33THY/Ultimate/raw/refs/heads/main/IWR.ps1 -useb | iex"
echo.
pause
