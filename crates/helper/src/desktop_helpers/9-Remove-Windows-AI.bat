@echo off
title Remove Windows AI components
echo Remove Windows AI by zoicware
echo Source: https://github.com/zoicware/RemoveWindowsAI
echo.
echo Strips Copilot, Recall, generative-AI Paint / Photos / Notepad
echo features, AI-powered Search/Cortana hooks, and related AI
echo telemetry components from a Windows 11 install.
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm 'https://raw.githubusercontent.com/zoicware/RemoveWindowsAI/main/RemoveWindowsAi.ps1')))"
echo.
pause
