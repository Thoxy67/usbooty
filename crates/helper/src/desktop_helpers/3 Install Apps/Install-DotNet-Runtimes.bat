@echo off
title Install .NET Desktop Runtimes

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
echo .NET Desktop Runtime — LTS (8) and current (9), x64
echo.
echo Lots of modern desktop apps (WPF / WinForms) need the .NET Desktop
echo Runtime, the .NET-era complement to the Visual C++ Redistributable.
echo This installs both the supported LTS (8) and the current (9) x64
echo runtimes via winget.
echo.
echo This uses winget. If winget is missing, run Install-Winget.bat
echo first and then come back to this one.
echo.
echo Installing .NET Desktop Runtime 8 (LTS) ...
winget install --exact --id Microsoft.DotNet.DesktopRuntime.8 --accept-source-agreements --accept-package-agreements
echo.
echo Installing .NET Desktop Runtime 9 ...
winget install --exact --id Microsoft.DotNet.DesktopRuntime.9 --accept-source-agreements --accept-package-agreements
echo.
echo Both runtimes installed (or already up to date).
echo.
pause
