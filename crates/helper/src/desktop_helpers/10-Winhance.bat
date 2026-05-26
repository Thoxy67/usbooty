@echo off
title Winhance
echo Winhance - Windows enhancement utility
echo Source: https://github.com/memstechtips/Winhance
echo.
echo GUI tool for debloating, optimising, customising and securing
echo Windows 10 / 11 (removes bloat apps, tweaks privacy / telemetry
echo settings, manages Microsoft Edge, etc.).
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm 'https://get.winhance.net' | iex"
echo.
pause
