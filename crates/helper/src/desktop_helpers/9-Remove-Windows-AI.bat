@echo off
title Remove Windows AI components

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
echo Remove Windows AI by zoicware
echo Source: https://github.com/zoicware/RemoveWindowsAI
echo.
echo Strips Copilot, Recall, generative-AI Paint / Photos / Notepad
echo features, AI-powered Search/Cortana hooks, and related AI
echo telemetry components from a Windows 11 install.
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm 'https://raw.githubusercontent.com/zoicware/RemoveWindowsAI/main/RemoveWindowsAi.ps1')))"
echo.
pause
