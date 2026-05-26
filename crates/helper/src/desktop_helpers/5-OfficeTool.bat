@echo off
title Download OfficeTool
set "OUT=%USERPROFILE%\Downloads\OfficeTool.zip"
echo OfficeTool runtime download
echo Source: https://otp.landian.vip/
echo.
echo Saving to: %OUT%
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-WebRequest -Uri 'https://otp.landian.vip/redirect/download.php?type=runtime&arch=x64' -OutFile '%OUT%'"
echo.
if exist "%OUT%" (
  echo Download finished.
  echo Right-click the .zip in Explorer, Extract All..., then run OfficeTool.exe.
) else (
  echo Download FAILED — check your internet connection and try again.
)
echo.
pause
