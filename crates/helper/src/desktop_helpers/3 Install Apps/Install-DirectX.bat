@echo off
title Install DirectX Runtime

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
echo Microsoft DirectX Runtime (End-User Web Installer)
echo.
echo Installs the legacy D3DX, D3DCompiler and XAudio2 runtime DLLs that
echo a lot of older PC games still link against. Windows 10/11 already
echo ships modern DirectX 11/12, but games built with the DirectX SDK
echo from the 2000s/early 2010s expect these legacy libraries and crash
echo without them.
echo.
echo This uses winget. If winget is missing, run Install-Winget.bat
echo first and then come back to this one.
echo.
winget install --exact --id Microsoft.DirectX --accept-source-agreements --accept-package-agreements
echo.
echo Done. A reboot is not required.
echo.
pause
