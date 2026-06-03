@echo off
setlocal
title Install OfficeTool Plus

REM Self-elevate if not already running as administrator. `fltmc` is a
REM lightweight privileged-only command - a non-zero exit means we are
REM running unelevated. Relaunch via PowerShell's RunAs verb (which
REM triggers the UAC prompt) and exit the current low-priv window.
>nul 2>&1 fltmc
if not "%errorlevel%"=="0" (
    echo Requesting administrator privileges ...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

echo OfficeTool Plus  -  https://otp.landian.vip/
echo Mirror:           https://github.com/YerongAI/Office-Tool
echo.
echo Downloads OfficeTool, extracts it to your Desktop as an OfficeTool
echo folder, unblocks every file, and opens the folder in Explorer so
echo you can pick "Office Tool Plus.exe" yourself. Nothing is left in
echo %%TEMP%% afterwards.
echo.

REM Single long PowerShell -Command line. Quoting rules: cmd leaves
REM everything between the outer double-quotes alone (the &, |, (, )
REM and ; characters are PowerShell syntax here, not cmd), and every
REM string literal inside uses single quotes so we never need to
REM escape a double-quote.
REM
REM Layout of the upstream archive (Office_Tool_with_runtime_v*_x64.zip):
REM     Office Tool\
REM         Office Tool Plus.exe          <- the GUI
REM         Office Tool Plus.Console.exe  <- CLI variant
REM         files\setup.exe               <- Microsoft ODT
REM         shared\Microsoft.NETCore.App\10.x\createdump.exe
REM We pull the single top-level directory (whatever its name is), rename
REM it on the Desktop to "OfficeTool", then hand off to Explorer.
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; $url='https://otp.landian.vip/redirect/download.php?type=runtime&arch=x64'; $zip=Join-Path $env:TEMP 'OfficeTool.zip'; $stage=Join-Path $env:TEMP 'OfficeTool-extract'; $desktop=(New-Object -ComObject Shell.Application).Namespace('shell:Desktop').Self.Path; $dest=Join-Path $desktop 'OfficeTool'; if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }; Write-Host 'Downloading OfficeTool ...'; Invoke-WebRequest -Uri $url -OutFile $zip; Write-Host 'Extracting ...'; Expand-Archive -Path $zip -DestinationPath $stage -Force; $inner = Get-ChildItem -Path $stage -Directory | Select-Object -First 1; if (-not $inner) { $inner = Get-Item $stage }; if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }; Move-Item -Path $inner.FullName -Destination $dest -Force; Get-ChildItem -Path $dest -Recurse | Unblock-File; Remove-Item $zip -ErrorAction SilentlyContinue; Remove-Item $stage -Recurse -ErrorAction SilentlyContinue; Write-Host ('Opening ' + $dest + ' in Explorer ...'); Start-Process -FilePath explorer.exe -ArgumentList $dest"

echo.
pause
endlocal
