@echo off
title Disable Fast Startup

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
echo Disable Windows Fast Startup
echo.
echo Fast Startup hibernates the kernel + drivers on shutdown so the next
echo boot is faster. The downside: the disk's NTFS journal is left in a
echo dirty state, which makes the partition mount read-only (or risks
echo corruption) when another OS - a Linux dual-boot, a recovery USB -
echo accesses the same drive.
echo.
echo This .bat clears the Fast Startup flag but keeps hibernation itself
echo available, so "Hibernate" still works from the Start menu power list.
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Power" /v HiberbootEnabled /t REG_DWORD /d 0 /f
echo.
echo Done. Fast Startup is disabled. No reboot is required, but the change
echo only takes effect at the next shutdown.
echo.
pause
