@echo off
title FR33THY Ultimate Windows Optimization
echo FR33THY's Ultimate Windows Optimization
echo Source: https://github.com/FR33THYFR33THY/Ultimate
echo.
echo Gaming / latency-focused tweaks: power plan, scheduler, network
echo stack, GPU driver settings, services trim, and a long list of
echo registry optimisations targeted at minimising input latency and
echo frame-time spikes.
echo.
echo Run as administrator (right-click this .bat -^> "Run as administrator").
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "iwr https://github.com/FR33THYFR33THY/Ultimate/raw/refs/heads/main/IWR.ps1 -useb | iex"
echo.
pause
