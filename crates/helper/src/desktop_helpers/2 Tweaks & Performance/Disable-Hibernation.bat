@echo off
title Disable Hibernation

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
echo Disable hibernation (powercfg -h off)
echo.
echo Turns hibernation off and DELETES hiberfil.sys, freeing disk space worth
echo a large fraction of your RAM (often 6-13 GB). This also turns off Fast
echo Startup, which depends on hibernation. Good for SSDs, VMs, and dual-boot.
echo You lose the "Hibernate" power option and resume-from-hibernate.
echo.
echo (If you want to keep Hibernate available and only clear Fast Startup,
echo use Disable-FastStartup.bat instead.)
echo.
powercfg -h off
echo.
echo Done. hiberfil.sys removed and hibernation disabled. Re-enable any time
echo with: powercfg -h on
echo.
pause
