@echo off
title Win11Debloat (Raphire)
echo Win11Debloat by Raphire
echo Source: https://github.com/Raphire/Win11Debloat
echo.
echo Fetching and running the script from https://debloat.raphi.re/ ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm 'https://debloat.raphi.re/')))"
echo.
pause
