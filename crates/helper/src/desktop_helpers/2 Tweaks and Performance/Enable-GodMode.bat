@echo off
title Create God Mode folder on the Desktop

REM No elevation: this creates a folder in your own user profile's Desktop,
REM so it must run as you, not as an administrator.
echo Create the "God Mode" (All Tasks) folder on your Desktop
echo.
echo God Mode is a single folder that lists every Control Panel / settings
echo task in one place, handy for quick access to the classic applets. This
echo just creates the special folder on your Desktop; no admin needed.
echo.
mkdir "%USERPROFILE%\Desktop\GodMode.{ED7BA470-8E54-465E-825C-99712043E01C}" 2>nul
echo.
echo Done. Open the "GodMode" icon on your Desktop. To remove it, just delete
echo that folder.
echo.
pause
