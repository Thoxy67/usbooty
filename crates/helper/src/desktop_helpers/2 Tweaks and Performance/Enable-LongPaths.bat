@echo off
title Enable NTFS Long Paths (260+ characters)

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
echo Enable Win32 long path support
echo.
echo Lifts the historic 260-character path limit (MAX_PATH) for applications
echo that opt in via their manifest - modern Git, Node.js / npm, Python,
echo Rust, .NET 6+, and PowerShell already opt in. Without this key,
echo `node_modules` trees, deep Python venvs, and Git checkouts with long
echo branch names hit cryptic "filename too long" errors.
echo.
echo A handful of older line-of-business apps may misbehave with longer
echo paths; if you do not need long-path support, leave the system default.
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
reg add "HKLM\SYSTEM\CurrentControlSet\Control\FileSystem" /v LongPathsEnabled /t REG_DWORD /d 1 /f
echo.
echo Done. The flag is read on the next sign-in; a reboot guarantees every
echo process picks it up.
echo.
pause
