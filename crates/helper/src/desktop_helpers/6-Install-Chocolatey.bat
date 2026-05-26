@echo off
title Install Chocolatey
echo Chocolatey - the Windows package manager
echo Source: https://chocolatey.org/install
echo.
echo This needs to run as Administrator. Right-click this .bat file and
echo choose "Run as administrator" if you didn't already.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))"
echo.
echo If install succeeded, open a new terminal and try: choco --version
echo.
pause
