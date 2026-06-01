@echo off
title Restore the classic (Windows 10) right-click menu

REM No elevation: the key lives under HKCU, so this must run as you, not as
REM an administrator (an elevated shell would write a different user's hive).
echo Restore the classic Windows 10 right-click context menu on Windows 11
echo.
echo Replaces the trimmed Windows 11 right-click menu with the full classic
echo one, so you no longer have to click "Show more options". Per-user; no
echo admin needed. Explorer restarts to apply it.
echo.
reg add "HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32" /ve /t REG_SZ /d "" /f
echo.
echo Restarting Explorer to apply ...
taskkill /f /im explorer.exe >nul 2>&1
start explorer.exe
echo.
echo Done. To revert: delete the CLSID key
echo (reg delete "HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}" /f)
echo and restart Explorer.
echo.
pause
