@echo off
title Remove OneDrive
echo Killing the OneDrive process if running...
taskkill /f /im OneDrive.exe 2>nul
echo.
echo Running the 64-bit OneDrive uninstaller...
"%SystemRoot%\System32\OneDriveSetup.exe" /uninstall
echo.
echo Running the 32-bit (WoW64) OneDrive uninstaller...
"%SystemRoot%\SysWOW64\OneDriveSetup.exe" /uninstall
echo.
echo OneDrive removal finished. You may need to sign out / restart for the
echo OneDrive shell entry to fully disappear.
echo.
pause
