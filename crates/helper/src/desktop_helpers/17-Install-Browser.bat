@echo off
setlocal enabledelayedexpansion
title Install a browser

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

:menu
cls
echo ================================================
echo  USBooty browser installer
echo ================================================
echo.
echo   1)  Google Chrome      (direct Google installer)
echo   2)  Mozilla Firefox    (winget: Mozilla.Firefox)
echo   3)  Brave              (winget: Brave.Brave)
echo   4)  Zen Browser        (winget: Zen-Team.Zen-Browser)
echo   5)  LibreWolf          (winget: LibreWolf.LibreWolf)
echo   6)  Floorp             (winget: Ablaze.Floorp)
echo   7)  Waterfox           (winget: Waterfox.Waterfox)
echo   8)  Opera              (winget: Opera.Opera)
echo   9)  Opera GX           (winget: Opera.OperaGX)
echo   A)  Vivaldi            (winget: Vivaldi.Vivaldi)
echo   B)  Arc Browser        (winget: TheBrowserCompany.Arc)
echo.
echo   0)  Quit
echo.
echo Press one key to start an install. Anything else just refreshes
echo this menu. Every winget option needs winget — if it's missing,
echo run 8-Install-Winget.bat first.
echo.

REM `choice` accepts one keystroke and only returns when it matches a
REM character from /c. Any other key beeps and waits, which is the
REM single-key-menu pattern Massgrave's activator uses. ERRORLEVEL is
REM the 1-based index of the matched char inside /c.
choice /c 123456789AB0 /n /m "> "
set "k=!errorlevel!"

if "!k!"=="1"  ( call :chrome    & goto menu )
if "!k!"=="2"  ( call :firefox   & goto menu )
if "!k!"=="3"  ( call :brave     & goto menu )
if "!k!"=="4"  ( call :zen       & goto menu )
if "!k!"=="5"  ( call :librewolf & goto menu )
if "!k!"=="6"  ( call :floorp    & goto menu )
if "!k!"=="7"  ( call :waterfox  & goto menu )
if "!k!"=="8"  ( call :opera     & goto menu )
if "!k!"=="9"  ( call :operagx   & goto menu )
if "!k!"=="10" ( call :vivaldi   & goto menu )
if "!k!"=="11" ( call :arc       & goto menu )
if "!k!"=="12" goto end

REM Unreachable in normal flow (choice only returns matched keys), but
REM Ctrl+C / Ctrl+Break shows up as errorlevel 0 / 255 — redraw rather
REM than exit so a stray interrupt doesn't drop the user out.
goto menu

:chrome
echo.
echo Downloading and installing Google Chrome from dl.google.com ...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$uri = [uri]::new('https://dl.google.com/chrome/install/chrome_installer.exe'); $file = Join-Path $env:TEMP $uri.Segments[-1]; [System.Net.WebClient]::new().DownloadFile($uri, $file); Start-Process -FilePath $file -ArgumentList '/silent /install' -Wait; Remove-Item -LiteralPath $file -ErrorAction 'SilentlyContinue';"
echo.
pause
exit /b

:firefox
echo.
winget install --exact --id Mozilla.Firefox --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:brave
echo.
winget install --exact --id Brave.Brave --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:zen
echo.
winget install --exact --id Zen-Team.Zen-Browser --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:librewolf
echo.
winget install --exact --id LibreWolf.LibreWolf --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:floorp
echo.
winget install --exact --id Ablaze.Floorp --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:waterfox
echo.
winget install --exact --id Waterfox.Waterfox --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:opera
echo.
winget install --exact --id Opera.Opera --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:operagx
echo.
winget install --exact --id Opera.OperaGX --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:vivaldi
echo.
winget install --exact --id Vivaldi.Vivaldi --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:arc
echo.
winget install --exact --id TheBrowserCompany.Arc --accept-source-agreements --accept-package-agreements
echo.
pause
exit /b

:end
endlocal
exit /b
