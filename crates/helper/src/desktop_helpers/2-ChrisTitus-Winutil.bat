@echo off
title Chris Titus Tech - Windows Utility (stable)
echo Chris Titus Tech Windows Utility (stable channel)
echo Source: https://github.com/ChrisTitusTech/winutil
echo.
echo Fetching and running the script from https://christitus.com/win ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm 'https://christitus.com/win' | iex"
echo.
pause
