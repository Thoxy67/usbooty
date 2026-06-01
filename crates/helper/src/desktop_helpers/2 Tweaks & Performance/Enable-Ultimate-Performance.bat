@echo off
title Enable Ultimate Performance power plan

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
echo Enable the "Ultimate Performance" power plan
echo.
echo Unlocks and activates Windows' hidden Ultimate Performance plan, which
echo trims micro-latencies from power-state transitions. Best on desktops or
echo plugged-in machines; on a laptop on battery it raises power draw.
echo.
powercfg -duplicatescheme e9a42b02-d5df-448d-aa00-03f14749eb61
powercfg /setactive e9a42b02-d5df-448d-aa00-03f14749eb61
echo.
echo Done. Confirm under Control Panel -^> Power Options. To revert, pick
echo Balanced again.
echo.
pause
