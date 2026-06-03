@echo off
title Disable Xbox Game Bar / Game DVR

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
echo Disable Xbox Game Bar and Game DVR (background game recording)
echo.
echo Game DVR records gameplay in the background and can add input latency
echo and frame-time spikes. This turns off the Game Bar and DVR capture for
echo your account and machine-wide. (Elevation keeps your user context, so
echo the HKCU keys still target your profile.)
echo.
reg add "HKCU\System\GameConfigStore" /v GameDVR_Enabled /t REG_DWORD /d 0 /f
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR" /v AppCaptureEnabled /t REG_DWORD /d 0 /f
reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows\GameDVR" /v AllowGameDVR /t REG_DWORD /d 0 /f
echo.
echo Done. Sign out / reboot for it to fully apply.
echo.
pause
