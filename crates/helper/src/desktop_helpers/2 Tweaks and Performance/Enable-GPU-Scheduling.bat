@echo off
title Enable Hardware-accelerated GPU Scheduling

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
echo Enable Hardware-accelerated GPU Scheduling (HAGS)
echo.
echo Lets the GPU manage its own VRAM scheduling, which can cut latency and
echo CPU overhead on supported GPUs (recent NVIDIA / AMD / Intel). It is a
echo no-op on hardware or drivers that don't support it.
echo.
reg add "HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" /v HwSchMode /t REG_DWORD /d 2 /f
echo.
echo Done. A REBOOT is required for this to take effect. To revert, set
echo HwSchMode back to 1.
echo.
pause
