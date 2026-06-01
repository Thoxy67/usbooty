@echo off
title Install Scoop
echo Scoop - command-line installer for Windows
echo Source: https://scoop.sh
echo.
echo NOTE: Scoop is installed PER-USER and must NOT be run as administrator.
echo If you opened this with "Run as administrator", close it and double-click
echo this .bat as your normal account instead.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser -Force; Invoke-RestMethod -Uri https://get.scoop.sh | Invoke-Expression"
echo.
echo If install succeeded, open a new terminal and try: scoop --version
echo.
pause
