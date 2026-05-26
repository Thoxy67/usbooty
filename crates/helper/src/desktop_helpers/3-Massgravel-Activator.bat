@echo off
title Microsoft Activation Scripts (Massgrave)
echo Microsoft Activation Scripts (MAS) by Massgrave
echo Source: https://github.com/massgravel/Microsoft-Activation-Scripts
echo.
echo Fetching and running the script from https://get.activated.win ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm 'https://get.activated.win' | iex"
echo.
pause
