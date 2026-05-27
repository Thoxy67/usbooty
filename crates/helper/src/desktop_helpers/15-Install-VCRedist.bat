@echo off
title Install Visual C++ Redistributables (2015-2022)

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
echo Visual C++ Runtime — 2015-2022 unified package, x64 and x86
echo.
echo Required by a large fraction of third-party software and games. The
echo "2015-2022" Redistributable is one merged runtime package that covers
echo every VC++ version from 2015 through 2022.
echo.
echo This uses winget. If winget is missing, run 8-Install-Winget.bat
echo first and then come back to this one.
echo.
echo Installing x64 ...
winget install --exact --id Microsoft.VCRedist.2015+.x64 --accept-source-agreements --accept-package-agreements
echo.
echo Installing x86 ...
winget install --exact --id Microsoft.VCRedist.2015+.x86 --accept-source-agreements --accept-package-agreements
echo.
echo Both runtimes installed (or already up to date).
echo.
pause
